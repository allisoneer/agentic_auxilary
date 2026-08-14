use attention_server::ServerConfig;
use attention_server::runtime;
use attention_turso::Config as TursoConfig;
use std::net::SocketAddr;
use std::path::PathBuf;

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is not set").into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server = ServerConfig::default();
    if let Ok(bind) = std::env::var("ATTENTION_BIND") {
        server.bind = bind.parse::<SocketAddr>()?;
    }
    server.allow_non_loopback = std::env::var_os("ATTENTION_ALLOW_NON_LOOPBACK").is_some();
    if let Ok(value) = std::env::var("ATTENTION_MAX_SOURCE_COMPONENT_BYTES") {
        server.max_source_component_bytes = value.parse()?;
    }
    if let Ok(value) = std::env::var("ATTENTION_MAX_SOURCE_ORDER_BYTES") {
        server.max_source_order_bytes = value.parse()?;
    }
    let database = PathBuf::from(required("ATTENTION_DATABASE_DIRECTORY")?);
    let backups = PathBuf::from(required("ATTENTION_BACKUP_DIRECTORY")?);
    let turso = TursoConfig::new(database, backups)?;
    runtime::run(server, turso).await?;
    Ok(())
}
