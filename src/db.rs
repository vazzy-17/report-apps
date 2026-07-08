use sqlx::{postgres::PgRow, Row, Column, TypeInfo, PgPool, postgres::PgPoolOptions};
use serde_json::{Value, Map};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc, NaiveDateTime, NaiveDate, NaiveTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct DbManager {
    pools: Arc<Mutex<HashMap<String, PgPool>>>,
}

impl DbManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_pool(&self, cache_key: &str, connection_string: &str) -> Result<PgPool, sqlx::Error> {
        let mut pools = self.pools.lock().await;
        if let Some(pool) = pools.get(cache_key) {
            if !pool.is_closed() {
                return Ok(pool.clone());
            }
        }

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(connection_string)
            .await?;

        pools.insert(cache_key.to_string(), pool.clone());
        Ok(pool)
    }
}

pub fn pg_row_to_json(row: &PgRow) -> Value {
    let mut map = Map::new();
    for col in row.columns() {
        let name = col.name();
        let type_name = col.type_info().name();
        
        let value = match type_name {
            "BOOL" | "boolean" => {
                row.try_get::<bool, _>(name).map(Value::Bool).unwrap_or(Value::Null)
            }
            "INT2" | "smallint" | "SMALLINT" => {
                row.try_get::<i16, _>(name).map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
            }
            "INT4" | "integer" | "INTEGER" | "SERIAL" => {
                row.try_get::<i32, _>(name).map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
            }
            "INT8" | "bigint" | "BIGINT" | "BIGSERIAL" => {
                row.try_get::<i64, _>(name).map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
            }
            "FLOAT4" | "real" | "REAL" => {
                row.try_get::<f32, _>(name)
                    .map(|v| serde_json::Number::from_f64(v as f64).map(Value::Number).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null)
            }
            "FLOAT8" | "double precision" | "DOUBLE PRECISION" => {
                row.try_get::<f64, _>(name)
                    .map(|v| serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null)
            }
            "NUMERIC" | "numeric" | "DECIMAL" | "decimal" => {
                row.try_get::<f64, _>(name)
                    .map(|v| serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "VARCHAR" | "text" | "TEXT" | "bpchar" | "char" | "name" => {
                row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
            }
            "TIMESTAMP" | "timestamp" | "TIMESTAMP WITHOUT TIME ZONE" => {
                row.try_get::<NaiveDateTime, _>(name)
                    .map(|v| Value::String(v.format("%Y-%m-%d %H:%M:%S").to_string()))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "TIMESTAMPTZ" | "timestamptz" | "TIMESTAMP WITH TIME ZONE" => {
                row.try_get::<DateTime<Utc>, _>(name)
                    .map(|v| Value::String(v.to_rfc3339()))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "DATE" | "date" => {
                row.try_get::<NaiveDate, _>(name)
                    .map(|v| Value::String(v.format("%Y-%m-%d").to_string()))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "TIME" | "time" => {
                row.try_get::<NaiveTime, _>(name)
                    .map(|v| Value::String(v.format("%H:%M:%S").to_string()))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "UUID" | "uuid" => {
                row.try_get::<Uuid, _>(name)
                    .map(|v| Value::String(v.to_string()))
                    .unwrap_or_else(|_| {
                        row.try_get::<String, _>(name).map(Value::String).unwrap_or(Value::Null)
                    })
            }
            "JSON" | "json" | "JSONB" | "jsonb" => {
                row.try_get::<Value, _>(name).unwrap_or(Value::Null)
            }
            _ => {
                if let Ok(v) = row.try_get::<String, _>(name) {
                    Value::String(v)
                } else if let Ok(v) = row.try_get::<i64, _>(name) {
                    Value::Number(v.into())
                } else if let Ok(v) = row.try_get::<f64, _>(name) {
                    serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
                } else if let Ok(v) = row.try_get::<bool, _>(name) {
                    Value::Bool(v)
                } else {
                    Value::Null
                }
            }
        };
        map.insert(name.to_string(), value);
    }
    Value::Object(map)
}
