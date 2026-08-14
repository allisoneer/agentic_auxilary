use attention_server::ServerConfig;
use attention_server::runtime;
use attention_turso::Config as TursoConfig;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is not set").into())
}

fn allow_non_loopback(value: Option<&str>) -> bool {
    value.is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(match EnvFilter::try_from_default_env() {
            Ok(filter) => filter,
            Err(_) => EnvFilter::new("warn"),
        })
        .init();

    let mut server = ServerConfig::default();
    if let Ok(bind) = std::env::var("ATTENTION_BIND") {
        server.bind = bind.parse::<SocketAddr>()?;
    }
    let allow_non_loopback_value = std::env::var("ATTENTION_ALLOW_NON_LOOPBACK").ok();
    server.allow_non_loopback = allow_non_loopback(allow_non_loopback_value.as_deref());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_loopback_exposure_requires_an_explicit_true_value() {
        for value in [Some("1"), Some("true"), Some("TRUE"), Some("TrUe")] {
            assert!(allow_non_loopback(value));
        }
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("yes"),
            Some(" true "),
        ] {
            assert!(!allow_non_loopback(value));
        }
    }
}
