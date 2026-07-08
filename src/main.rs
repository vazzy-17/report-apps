use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use rand::Rng;

mod config;
mod crypto;
mod db;
mod handlers;

use handlers::{get_servers, login, list_tables, get_table_schema, get_table_data, execute_query, AppState};
use db::DbManager;

#[tokio::main]
async fn main() {
    println!("Initializing server...");

    // Generate or load encryption key (32 bytes)
    let mut encryption_key = [0u8; 32];
    if let Ok(key_str) = std::env::var("ENCRYPTION_KEY") {
        let bytes = key_str.as_bytes();
        let len = bytes.len().min(32);
        encryption_key[..len].copy_from_slice(&bytes[..len]);
    } else {
        rand::thread_rng().fill(&mut encryption_key);
    }

    // Generate or load JWT secret
    let jwt_secret = std::env::var("JWT_SECRET")
        .map(|s| s.into_bytes())
        .unwrap_or_else(|_| {
            let mut secret = vec![0u8; 32];
            rand::thread_rng().fill(&mut secret[..]);
            secret
        });

    let db_manager = DbManager::new();

    let state = AppState {
        db_manager,
        encryption_key,
        jwt_secret,
    };

    // Create the router
    let app = Router::new()
        // API routes
        .route("/api/servers", get(get_servers))
        .route("/api/auth/login", post(login))
        .route("/api/tables", get(list_tables))
        .route("/api/table/:table_name/schema", get(get_table_schema))
        .route("/api/table/:table_name/data", get(get_table_data))
        .route("/api/query", post(execute_query))
        .with_state(state)
        // Serve static frontend files
        .fallback_service(ServeDir::new("static"))
        // CORS
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], 7272));
    println!("Server running on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
