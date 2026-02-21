use serde::{de, Deserialize, Deserializer};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct DevrigConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
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
}
