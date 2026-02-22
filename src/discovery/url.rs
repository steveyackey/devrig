use crate::config::model::InfraConfig;

/// Generate a connection URL for an infrastructure service based on its image type
/// and resolved port.
///
/// Rules:
/// - postgres:// for Postgres images (with optional user:pass from env)
/// - redis:// for Redis images
/// - No protocol (just localhost:port) when the infra has named ports
/// - http:// as the default fallback
pub fn generate_url(name: &str, infra_config: &InfraConfig, port: u16) -> String {
    let _ = name; // reserved for future use

    if infra_config.image.starts_with("postgres") {
        let user = infra_config
            .env
            .get("POSTGRES_USER")
            .map(|s| s.as_str())
            .unwrap_or("postgres");
        let credentials = match infra_config.env.get("POSTGRES_PASSWORD") {
            Some(pass) => format!("{}:{}@", user, pass),
            None => format!("{}@", user),
        };
        return format!("postgres://{}localhost:{}", credentials, port);
    }

    if infra_config.image.starts_with("redis") {
        return format!("redis://localhost:{}", port);
    }

    if !infra_config.ports.is_empty() {
        return format!("localhost:{}", port);
    }

    format!("http://localhost:{}", port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{InfraConfig, Port};
    use std::collections::BTreeMap;

    fn base_infra(image: &str) -> InfraConfig {
        InfraConfig {
            image: image.to_string(),
            port: None,
            ports: BTreeMap::new(),
            env: BTreeMap::new(),
            volumes: Vec::new(),
            ready_check: None,
            init: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn postgres_url_with_credentials() {
        let mut infra = base_infra("postgres:16-alpine");
        infra.env.insert("POSTGRES_USER".into(), "devrig".into());
        infra
            .env
            .insert("POSTGRES_PASSWORD".into(), "secret".into());
        let url = generate_url("postgres", &infra, 5432);
        assert_eq!(url, "postgres://devrig:secret@localhost:5432");
    }

    #[test]
    fn postgres_url_without_password() {
        let mut infra = base_infra("postgres:16-alpine");
        infra.env.insert("POSTGRES_USER".into(), "myuser".into());
        let url = generate_url("postgres", &infra, 5432);
        assert_eq!(url, "postgres://myuser@localhost:5432");
    }

    #[test]
    fn postgres_url_defaults_user() {
        let infra = base_infra("postgres:16");
        let url = generate_url("pg", &infra, 5432);
        assert_eq!(url, "postgres://postgres@localhost:5432");
    }

    #[test]
    fn redis_url() {
        let infra = base_infra("redis:7-alpine");
        let url = generate_url("redis", &infra, 6379);
        assert_eq!(url, "redis://localhost:6379");
    }

    #[test]
    fn http_default_url() {
        let infra = base_infra("minio/minio:latest");
        let url = generate_url("minio", &infra, 9000);
        assert_eq!(url, "http://localhost:9000");
    }

    #[test]
    fn multi_port_no_protocol() {
        let mut infra = base_infra("axllent/mailpit:latest");
        infra.ports.insert("smtp".into(), Port::Fixed(1025));
        infra.ports.insert("ui".into(), Port::Fixed(8025));
        let url = generate_url("mailpit", &infra, 1025);
        assert_eq!(url, "localhost:1025");
    }
}
