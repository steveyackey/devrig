use crate::config::model::DockerConfig;

/// Generate a connection URL for an infrastructure service based on its image type
/// and resolved port.
///
/// Rules:
/// - postgres:// for Postgres images (with optional user:pass from env)
/// - redis:// for Redis images
/// - No protocol (just localhost:port) when the docker service has named ports
/// - http:// as the default fallback
pub fn generate_url(name: &str, docker_config: &DockerConfig, port: u16) -> String {
    let _ = name; // reserved for future use

    if docker_config.image.starts_with("postgres") {
        let user = docker_config
            .env
            .get("POSTGRES_USER")
            .map(|s| s.as_str())
            .unwrap_or("postgres");
        let credentials = match docker_config.env.get("POSTGRES_PASSWORD") {
            Some(pass) => format!("{}:{}@", user, pass),
            None => format!("{}@", user),
        };
        return format!("postgres://{}localhost:{}", credentials, port);
    }

    if docker_config.image.starts_with("redis") {
        return format!("redis://localhost:{}", port);
    }

    if !docker_config.ports.is_empty() {
        return format!("localhost:{}", port);
    }

    format!("http://localhost:{}", port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{DockerConfig, Port};
    use std::collections::BTreeMap;

    fn base_infra(image: &str) -> DockerConfig {
        DockerConfig {
            image: image.to_string(),
            port: None,
            ports: BTreeMap::new(),
            env: BTreeMap::new(),
            volumes: Vec::new(),
            command: None,
            entrypoint: None,
            ready_check: None,
            init: Vec::new(),
            depends_on: Vec::new(),
            registry_auth: None,
        }
    }

    #[test]
    fn postgres_url_with_credentials() {
        let mut cfg = base_infra("postgres:16-alpine");
        cfg.env.insert("POSTGRES_USER".into(), "devrig".into());
        cfg
            .env
            .insert("POSTGRES_PASSWORD".into(), "secret".into());
        let url = generate_url("postgres", &cfg, 5432);
        assert_eq!(url, "postgres://devrig:secret@localhost:5432");
    }

    #[test]
    fn postgres_url_without_password() {
        let mut cfg = base_infra("postgres:16-alpine");
        cfg.env.insert("POSTGRES_USER".into(), "myuser".into());
        let url = generate_url("postgres", &cfg, 5432);
        assert_eq!(url, "postgres://myuser@localhost:5432");
    }

    #[test]
    fn postgres_url_defaults_user() {
        let cfg = base_infra("postgres:16");
        let url = generate_url("pg", &cfg, 5432);
        assert_eq!(url, "postgres://postgres@localhost:5432");
    }

    #[test]
    fn redis_url() {
        let cfg = base_infra("redis:7-alpine");
        let url = generate_url("redis", &cfg, 6379);
        assert_eq!(url, "redis://localhost:6379");
    }

    #[test]
    fn http_default_url() {
        let cfg = base_infra("minio/minio:latest");
        let url = generate_url("minio", &cfg, 9000);
        assert_eq!(url, "http://localhost:9000");
    }

    #[test]
    fn multi_port_no_protocol() {
        let mut cfg = base_infra("axllent/mailpit:latest");
        cfg.ports.insert("smtp".into(), Port::Fixed(1025));
        cfg.ports.insert("ui".into(), Port::Fixed(8025));
        let url = generate_url("mailpit", &cfg, 1025);
        assert_eq!(url, "localhost:1025");
    }
}
