use axum::{
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{load_databases, DatabaseServer};
use crate::crypto::{encrypt, decrypt};
use crate::db::{pg_row_to_json, DbManager};

#[derive(Clone)]
pub struct AppState {
    pub db_manager: DbManager,
    pub encryption_key: [u8; 32],
    pub jwt_secret: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub username: String,
    pub encrypted_password: String,
    pub exp: usize,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub server_id: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub dbname: String,
    pub server_name: String,
}

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

pub async fn get_servers() -> Result<Json<Vec<DatabaseServer>>, (StatusCode, Json<Value>)> {
    let servers = load_databases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to load databases config: {}", e)}))))?;
    Ok(Json(servers))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<Value>)> {
    let servers = load_databases()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to load databases config: {}", e)}))))?;
        
    let server = servers.iter().find(|s| s.id == payload.server_id)
        .ok_or((StatusCode::BAD_REQUEST, Json(json!({"error": "Selected server not found"}))))?;

    let username = server.username.clone();
    let password = server.password.clone().unwrap_or_default();
    
    // Construct database URL to test the connection
    let conn_str = if password.is_empty() {
        format!("postgresql://{}@{}:{}/{}", username, server.host, server.port, server.dbname)
    } else {
        format!("postgresql://{}:{}@{}:{}/{}", username, urlencoding::encode(&password), server.host, server.port, server.dbname)
    };

    // Test connect
    let test_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&conn_str)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, Json(json!({"error": format!("Database login failed: {}", e)}))))?;
        
    // Close the test connection pool
    test_pool.close().await;

    // Encrypt password
    let encrypted_password = if password.is_empty() {
        String::new()
    } else {
        encrypt(&password, &state.encryption_key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to encrypt credentials: {}", e)}))))?
    };

    // Generate JWT
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize + 24 * 3600; // 24 hours

    let claims = Claims {
        server_id: server.id.clone(),
        host: server.host.clone(),
        port: server.port,
        dbname: server.dbname.clone(),
        username: username.clone(),
        encrypted_password,
        exp,
    };

    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&state.jwt_secret)
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to generate session token: {}", e)}))))?;

    Ok(Json(LoginResponse {
        token,
        username,
        dbname: server.dbname.clone(),
        server_name: server.name.clone(),
    }))
}

// Helper to authenticate request and get the pool
pub async fn get_pool_from_auth(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<(sqlx::PgPool, String), (StatusCode, Json<Value>)> {
    let auth_header = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, Json(json!({"error": "Missing Authorization header"}))))?;
        
    if !auth_header.starts_with("Bearer ") {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "Invalid Authorization header format"}))));
    }
    
    let token = &auth_header[7..];
    
    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(&state.jwt_secret),
        &jsonwebtoken::Validation::default()
    ).map_err(|e| (StatusCode::UNAUTHORIZED, Json(json!({"error": format!("Invalid token: {}", e)}))))?;
    
    let claims = token_data.claims;
    
    // Decrypt password
    let password = if claims.encrypted_password.is_empty() {
        String::new()
    } else {
        decrypt(&claims.encrypted_password, &state.encryption_key)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Decryption failed: {}", e)}))))?
    };
    
    // Construct connection string
    let conn_str = if password.is_empty() {
        format!("postgresql://{}@{}:{}/{}", claims.username, claims.host, claims.port, claims.dbname)
    } else {
        format!("postgresql://{}:{}@{}:{}/{}", claims.username, urlencoding::encode(&password), claims.host, claims.port, claims.dbname)
    };
    
    let cache_key = format!("{}:{}:{}:{}", claims.host, claims.port, claims.dbname, claims.username);
    
    let pool = state.db_manager.get_pool(&cache_key, &conn_str).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to connect to database: {}", e)}))))?;
        
    Ok((pool, claims.server_id))
}

pub async fn list_tables(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let (pool, _) = get_pool_from_auth(&headers, &state).await?;

    // Fetch tables, types, sizes and estimated row counts
    let query_str = r#"
        SELECT 
            c.relname AS table_name,
            CASE WHEN c.relkind = 'r' THEN 'BASE TABLE' ELSE 'VIEW' END AS table_type,
            pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size,
            c.reltuples::bigint AS row_count
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public' AND c.relkind IN ('r', 'v')
        ORDER BY c.relname;
    "#;

    let rows = sqlx::query(query_str)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Query failed: {}", e)}))))?;

    let tables: Vec<Value> = rows.iter().map(|r| {
        json!({
            "table_name": r.try_get::<String, _>("table_name").unwrap_or_default(),
            "table_type": r.try_get::<String, _>("table_type").unwrap_or_default(),
            "total_size": r.try_get::<String, _>("total_size").unwrap_or_default(),
            "row_count": r.try_get::<i64, _>("row_count").unwrap_or_default(),
        })
    }).collect();

    Ok(Json(json!(tables)))
}

pub async fn get_table_schema(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(table_name): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let (pool, _) = get_pool_from_auth(&headers, &state).await?;

    // Security check: Verify that this table exists in public schema to prevent SQL Injection
    let verify_query = "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1)";
    let exists: bool = sqlx::query_scalar(verify_query)
        .bind(&table_name)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Database validation failed: {}", e)}))))?;

    if !exists {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Table does not exist or access is restricted"}))));
    }

    let schema_query = r#"
        SELECT 
            column_name, 
            data_type, 
            is_nullable,
            column_default
        FROM information_schema.columns 
        WHERE table_schema = 'public' AND table_name = $1
        ORDER BY ordinal_position;
    "#;

    let rows = sqlx::query(schema_query)
        .bind(&table_name)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Query failed: {}", e)}))))?;

    let schema: Vec<Value> = rows.iter().map(|r| {
        json!({
            "column_name": r.try_get::<String, _>("column_name").unwrap_or_default(),
            "data_type": r.try_get::<String, _>("data_type").unwrap_or_default(),
            "is_nullable": r.try_get::<String, _>("is_nullable").unwrap_or_default(),
            "column_default": r.try_get::<Option<String>, _>("column_default").unwrap_or_default(),
        })
    }).collect();

    Ok(Json(json!(schema)))
}

pub async fn get_table_data(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(table_name): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let (pool, _) = get_pool_from_auth(&headers, &state).await?;

    // Security check: Verify table name
    let verify_query = "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1)";
    let exists: bool = sqlx::query_scalar(verify_query)
        .bind(&table_name)
        .fetch_one(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Database validation failed: {}", e)}))))?;

    if !exists {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Table does not exist or access is restricted"}))));
    }

    // Limit to 200 rows for view table data
    let query_str = format!("SELECT * FROM public.{} LIMIT 200", table_name);

    let rows = sqlx::query(&query_str)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Query execution failed: {}", e)}))))?;

    let data: Vec<Value> = rows.iter().map(pg_row_to_json).collect();

    Ok(Json(json!(data)))
}

pub async fn execute_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<QueryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let (pool, _) = get_pool_from_auth(&headers, &state).await?;

    // Basic SQL validation: Only SELECT, WITH or SHOW allowed
    let trimmed_query = payload.query.trim().to_uppercase();
    if !trimmed_query.starts_with("SELECT") && !trimmed_query.starts_with("WITH") && !trimmed_query.starts_with("SHOW") {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Only SELECT or read-only queries are allowed in reporting dashboard."}))));
    }

    let rows = sqlx::query(&payload.query)
        .fetch_all(&pool)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": format!("SQL Error: {}", e)}))))?;

    let data: Vec<Value> = rows.iter().map(pg_row_to_json).collect();

    Ok(Json(json!(data)))
}
