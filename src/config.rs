use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseServer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub username: String,
    pub password: Option<String>,
}

pub fn load_databases() -> Result<Vec<DatabaseServer>, Box<dyn std::error::Error>> {
    let path = Path::new("databases.json");
    println!("Current dir : {:?}", std::env::current_dir()?);
    println!("Database file exists : {}", path.exists());
    if !path.exists() {
        return Ok(vec![]);
    }
    
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let servers: Vec<DatabaseServer> = serde_json::from_str(&contents)?;
    Ok(servers)
}