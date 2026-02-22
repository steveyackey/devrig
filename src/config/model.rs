use serde::{de, Deserialize, Deserializer};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct DevrigConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceConfig>,
    #[serde(default)]
    pub infra: BTreeMap<String, InfraConfig>,
    #[serde(default)]
    pub compose: Option<ComposeConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub network: Option<NetworkConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub path: Option<String>,
    pub command: String,
    #[serde(default)]
    pub port: Option<Port>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfraConfig {
    pub image: String,
    #[serde(default)]
    pub port: Option<Port>,
    #[serde(default)]
    pub ports: BTreeMap<String, Port>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub ready_check: Option<ReadyCheck>,
    #[serde(default)]
    pub init: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ReadyCheck {
    #[serde(rename = "pg_isready")]
    PgIsReady,
    #[serde(rename = "cmd")]
    Cmd {
        command: String,
        #[serde(default)]
        expect: Option<String>,
    },
    #[serde(rename = "http")]
    Http { url: String },
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "log")]
    Log {
        #[serde(rename = "match")]
        pattern: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposeConfig {
    pub file: String,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub env_file: Option<String>,
    #[serde(default)]
    pub ready_checks: BTreeMap<String, ReadyCheck>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Port {
    Fixed(u16),
    Auto,
}

impl<'de> Deserialize<'de> for Port {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PortVisitor;

        impl<'de> de::Visitor<'de> for PortVisitor {
            type Value = Port;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a port number (1-65535) or the string \"auto\"")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                u16::try_from(v)
                    .map(Port::Fixed)
                    .map_err(|_| E::custom(format!("port {v} out of range (1-65535)")))
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                u16::try_from(v)
                    .map(Port::Fixed)
                    .map_err(|_| E::custom(format!("port {v} out of range (1-65535)")))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "auto" {
                    Ok(Port::Auto)
                } else {
                    Err(E::custom(format!("expected \"auto\" but got \"{v}\"")))
                }
            }
        }

        deserializer.deserialize_any(PortVisitor)
    }
}

impl Port {
    pub fn as_fixed(&self) -> Option<u16> {
        match self {
            Port::Fixed(p) => Some(*p),
            Port::Auto => None,
        }
    }

    pub fn is_auto(&self) -> bool {
        matches!(self, Port::Auto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml = r#"
            [project]
            name = "test"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.project.name, "test");
        assert!(config.services.is_empty());
        assert!(config.env.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let toml = r#"
            [project]
            name = "myapp"

            [env]
            RUST_LOG = "debug"
            DATABASE_URL = "postgres://localhost/myapp"

            [services.api]
            path = "./api"
            command = "cargo watch -x run"
            port = 3000
            depends_on = ["db"]

            [services.api.env]
            API_KEY = "secret"

            [services.web]
            command = "npm run dev"
            port = "auto"

            [services.db]
            command = "docker compose up postgres"
            port = 5432
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.project.name, "myapp");
        assert_eq!(config.services.len(), 3);
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.env["RUST_LOG"], "debug");

        let api = &config.services["api"];
        assert_eq!(api.path.as_deref(), Some("./api"));
        assert_eq!(api.command, "cargo watch -x run");
        assert!(matches!(api.port, Some(Port::Fixed(3000))));
        assert_eq!(api.depends_on, vec!["db"]);
        assert_eq!(api.env["API_KEY"], "secret");

        let web = &config.services["web"];
        assert!(matches!(web.port, Some(Port::Auto)));

        let db = &config.services["db"];
        assert!(matches!(db.port, Some(Port::Fixed(5432))));
        assert!(db.depends_on.is_empty());
    }

    #[test]
    fn parse_port_fixed() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
            port = 3000
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.services["api"].port,
            Some(Port::Fixed(3000))
        ));
    }

    #[test]
    fn parse_port_auto() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
            port = "auto"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(config.services["api"].port, Some(Port::Auto)));
    }

    #[test]
    fn parse_port_none() {
        let toml = r#"
            [project]
            name = "test"
            [services.worker]
            command = "echo hi"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.services["worker"].port.is_none());
    }

    #[test]
    fn parse_port_invalid_string() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
            port = "invalid"
        "#;
        let err = toml::from_str::<DevrigConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("expected \"auto\""));
    }

    #[test]
    fn parse_port_out_of_range() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
            port = 70000
        "#;
        let err = toml::from_str::<DevrigConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn parse_port_negative() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
            port = -1
        "#;
        let err = toml::from_str::<DevrigConfig>(toml).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn parse_missing_project_name() {
        let toml = r#"
            [project]
        "#;
        assert!(toml::from_str::<DevrigConfig>(toml).is_err());
    }

    #[test]
    fn parse_missing_project_section() {
        let toml = r#"
            [services.api]
            command = "echo hi"
        "#;
        assert!(toml::from_str::<DevrigConfig>(toml).is_err());
    }

    #[test]
    fn parse_missing_command() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            port = 3000
        "#;
        assert!(toml::from_str::<DevrigConfig>(toml).is_err());
    }

    #[test]
    fn parse_empty_services() {
        let toml = r#"
            [project]
            name = "test"
            [services]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.services.is_empty());
    }

    #[test]
    fn parse_service_with_all_fields() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            path = "./backend"
            command = "cargo run"
            port = 8080
            depends_on = ["db", "cache"]
            [services.api.env]
            PORT = "8080"
            HOST = "0.0.0.0"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let api = &config.services["api"];
        assert_eq!(api.path.as_deref(), Some("./backend"));
        assert_eq!(api.command, "cargo run");
        assert!(matches!(api.port, Some(Port::Fixed(8080))));
        assert_eq!(api.depends_on, vec!["db", "cache"]);
        assert_eq!(api.env.len(), 2);
    }

    #[test]
    fn parse_services_order_is_deterministic() {
        let toml = r#"
            [project]
            name = "test"
            [services.zebra]
            command = "echo z"
            [services.alpha]
            command = "echo a"
            [services.middle]
            command = "echo m"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let names: Vec<&String> = config.services.keys().collect();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn port_helper_methods() {
        assert_eq!(Port::Fixed(3000).as_fixed(), Some(3000));
        assert_eq!(Port::Auto.as_fixed(), None);
        assert!(!Port::Fixed(3000).is_auto());
        assert!(Port::Auto.is_auto());
    }

    // --- v0.2 InfraConfig tests ---

    #[test]
    fn parse_infra_single_port() {
        let toml = r#"
            [project]
            name = "test"

            [infra.postgres]
            image = "postgres:16-alpine"
            port = 5432
            [infra.postgres.env]
            POSTGRES_USER = "devrig"
            POSTGRES_PASSWORD = "devrig"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.infra.len(), 1);
        let pg = &config.infra["postgres"];
        assert_eq!(pg.image, "postgres:16-alpine");
        assert!(matches!(pg.port, Some(Port::Fixed(5432))));
        assert_eq!(pg.env["POSTGRES_USER"], "devrig");
    }

    #[test]
    fn parse_infra_named_ports() {
        let toml = r#"
            [project]
            name = "test"

            [infra.mailpit]
            image = "axllent/mailpit:latest"
            [infra.mailpit.ports]
            smtp = 1025
            ui = 8025
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let mp = &config.infra["mailpit"];
        assert_eq!(mp.image, "axllent/mailpit:latest");
        assert!(mp.port.is_none());
        assert_eq!(mp.ports.len(), 2);
        assert!(matches!(mp.ports["smtp"], Port::Fixed(1025)));
        assert!(matches!(mp.ports["ui"], Port::Fixed(8025)));
    }

    #[test]
    fn parse_infra_auto_port() {
        let toml = r#"
            [project]
            name = "test"
            [infra.redis]
            image = "redis:7-alpine"
            port = "auto"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(config.infra["redis"].port, Some(Port::Auto)));
    }

    #[test]
    fn parse_ready_check_pg_isready() {
        let toml = r#"
            [project]
            name = "test"
            [infra.postgres]
            image = "postgres:16"
            port = 5432
            ready_check = { type = "pg_isready" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.infra["postgres"].ready_check,
            Some(ReadyCheck::PgIsReady)
        ));
    }

    #[test]
    fn parse_ready_check_cmd() {
        let toml = r#"
            [project]
            name = "test"
            [infra.redis]
            image = "redis:7"
            port = 6379
            [infra.redis.ready_check]
            type = "cmd"
            command = "redis-cli ping"
            expect = "PONG"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.infra["redis"].ready_check {
            Some(ReadyCheck::Cmd { command, expect }) => {
                assert_eq!(command, "redis-cli ping");
                assert_eq!(expect.as_deref(), Some("PONG"));
            }
            other => panic!("expected ReadyCheck::Cmd, got {:?}", other),
        }
    }

    #[test]
    fn parse_ready_check_http() {
        let toml = r#"
            [project]
            name = "test"
            [infra.minio]
            image = "minio/minio"
            port = 9000
            ready_check = { type = "http", url = "http://localhost:9000/minio/health/live" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.infra["minio"].ready_check {
            Some(ReadyCheck::Http { url }) => {
                assert_eq!(url, "http://localhost:9000/minio/health/live");
            }
            other => panic!("expected ReadyCheck::Http, got {:?}", other),
        }
    }

    #[test]
    fn parse_ready_check_tcp() {
        let toml = r#"
            [project]
            name = "test"
            [infra.redis]
            image = "redis:7"
            port = 6379
            ready_check = { type = "tcp" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.infra["redis"].ready_check,
            Some(ReadyCheck::Tcp)
        ));
    }

    #[test]
    fn parse_ready_check_log() {
        let toml = r#"
            [project]
            name = "test"
            [infra.postgres]
            image = "postgres:16"
            port = 5432
            [infra.postgres.ready_check]
            type = "log"
            match = "ready to accept connections"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.infra["postgres"].ready_check {
            Some(ReadyCheck::Log { pattern }) => {
                assert_eq!(pattern, "ready to accept connections");
            }
            other => panic!("expected ReadyCheck::Log, got {:?}", other),
        }
    }

    #[test]
    fn parse_compose_config() {
        let toml = r#"
            [project]
            name = "test"

            [compose]
            file = "docker-compose.yml"
            services = ["redis", "postgres"]
            env_file = ".env"

            [compose.ready_checks.redis]
            type = "cmd"
            command = "redis-cli ping"
            expect = "PONG"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let compose = config.compose.unwrap();
        assert_eq!(compose.file, "docker-compose.yml");
        assert_eq!(compose.services, vec!["redis", "postgres"]);
        assert_eq!(compose.env_file.as_deref(), Some(".env"));
        assert_eq!(compose.ready_checks.len(), 1);
        assert!(matches!(
            compose.ready_checks["redis"],
            ReadyCheck::Cmd { .. }
        ));
    }

    #[test]
    fn parse_config_with_infra_and_services() {
        let toml = r#"
            [project]
            name = "myapp"

            [infra.postgres]
            image = "postgres:16-alpine"
            port = 5432
            [infra.postgres.env]
            POSTGRES_USER = "app"
            POSTGRES_PASSWORD = "secret"

            [infra.redis]
            image = "redis:7-alpine"
            port = 6379

            [services.api]
            command = "cargo run"
            port = 3000
            depends_on = ["postgres"]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.infra.len(), 2);
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.services["api"].depends_on, vec!["postgres"]);
    }

    #[test]
    fn parse_minimal_config_still_works() {
        // Backwards compatibility: v0.1 config with no infra/compose still works
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.infra.is_empty());
        assert!(config.compose.is_none());
        assert!(config.network.is_none());
    }

    #[test]
    fn parse_infra_with_all_fields() {
        let toml = r#"
            [project]
            name = "test"

            [infra.postgres]
            image = "postgres:16-alpine"
            port = 5432
            volumes = ["pgdata:/var/lib/postgresql/data"]
            init = [
                "CREATE DATABASE myapp;",
                "CREATE USER appuser WITH PASSWORD 'secret';",
            ]
            depends_on = ["redis"]

            [infra.postgres.env]
            POSTGRES_USER = "devrig"
            POSTGRES_PASSWORD = "devrig"

            [infra.postgres.ready_check]
            type = "pg_isready"

            [infra.redis]
            image = "redis:7-alpine"
            port = 6379
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let pg = &config.infra["postgres"];
        assert_eq!(pg.image, "postgres:16-alpine");
        assert!(matches!(pg.port, Some(Port::Fixed(5432))));
        assert_eq!(pg.volumes, vec!["pgdata:/var/lib/postgresql/data"]);
        assert_eq!(pg.init.len(), 2);
        assert_eq!(pg.depends_on, vec!["redis"]);
        assert!(matches!(pg.ready_check, Some(ReadyCheck::PgIsReady)));
        assert_eq!(pg.env.len(), 2);
    }

    #[test]
    fn parse_network_config() {
        let toml = r#"
            [project]
            name = "test"
            [network]
            name = "custom-net"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let net = config.network.unwrap();
        assert_eq!(net.name.as_deref(), Some("custom-net"));
    }
}
