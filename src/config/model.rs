use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct DevrigConfig {
    pub project: ProjectConfig,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceConfig>,
    #[serde(default)]
    pub docker: BTreeMap<String, DockerConfig>,
    #[serde(default)]
    pub compose: Option<ComposeConfig>,
    #[serde(default)]
    pub cluster: Option<ClusterConfig>,
    #[serde(default)]
    pub dashboard: Option<DashboardConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub network: Option<NetworkConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub env_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ServiceConfig {
    #[serde(default)]
    pub path: Option<String>,
    pub command: String,
    #[serde(default)]
    pub port: Option<Port>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_file: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub restart: Option<RestartConfig>,
}

fn default_restart_policy() -> String {
    "on-failure".to_string()
}

fn default_max_restarts() -> u32 {
    10
}

fn default_startup_max_restarts() -> u32 {
    3
}

fn default_startup_grace_ms() -> u64 {
    2000
}

fn default_initial_delay_ms() -> u64 {
    500
}

fn default_max_delay_ms() -> u64 {
    30000
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RestartConfig {
    #[serde(default = "default_restart_policy")]
    pub policy: String,
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    #[serde(default = "default_startup_max_restarts")]
    pub startup_max_restarts: u32,
    #[serde(default = "default_startup_grace_ms")]
    pub startup_grace_ms: u64,
    #[serde(default = "default_initial_delay_ms")]
    pub initial_delay_ms: u64,
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DockerConfig {
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
    pub command: Option<StringOrList>,
    #[serde(default)]
    pub entrypoint: Option<StringOrList>,
    #[serde(default)]
    pub ready_check: Option<ReadyCheck>,
    #[serde(default)]
    pub init: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub registry_auth: Option<RegistryAuth>,
}

/// A value that can be either a single string or a list of strings.
/// When given a string, it is kept as a single-element list.
#[derive(Debug, Clone, PartialEq)]
pub struct StringOrList(pub Vec<String>);

impl StringOrList {
    pub fn into_vec(self) -> Vec<String> {
        self.0
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for StringOrList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrListVisitor;

        impl<'de> de::Visitor<'de> for StringOrListVisitor {
            type Value = StringOrList;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or a list of strings")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<StringOrList, E> {
                Ok(StringOrList(vec![value.to_string()]))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<StringOrList, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<String>()? {
                    values.push(value);
                }
                Ok(StringOrList(values))
            }
        }

        deserializer.deserialize_any(StringOrListVisitor)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RegistryAuth {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ReadyCheck {
    #[serde(rename = "pg_isready")]
    PgIsReady {
        #[serde(default)]
        timeout: Option<u64>,
    },
    #[serde(rename = "cmd")]
    Cmd {
        command: String,
        #[serde(default)]
        expect: Option<String>,
        #[serde(default)]
        timeout: Option<u64>,
    },
    #[serde(rename = "http")]
    Http {
        url: String,
        #[serde(default)]
        timeout: Option<u64>,
    },
    #[serde(rename = "tcp")]
    Tcp {
        #[serde(default)]
        timeout: Option<u64>,
    },
    #[serde(rename = "log")]
    Log {
        #[serde(rename = "match")]
        pattern: String,
        #[serde(default)]
        timeout: Option<u64>,
    },
}

impl ReadyCheck {
    /// Get the configured timeout or return the default for this check type.
    pub fn timeout_secs(&self) -> u64 {
        let custom = match self {
            ReadyCheck::PgIsReady { timeout } => *timeout,
            ReadyCheck::Cmd { timeout, .. } => *timeout,
            ReadyCheck::Http { timeout, .. } => *timeout,
            ReadyCheck::Tcp { timeout } => *timeout,
            ReadyCheck::Log { timeout, .. } => *timeout,
        };
        custom.unwrap_or(match self {
            ReadyCheck::Log { .. } => 60,
            _ => 30,
        })
    }
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

fn default_dashboard_port() -> u16 {
    4000
}

fn default_grpc_port() -> u16 {
    4317
}

fn default_http_port() -> u16 {
    4318
}

fn default_trace_buffer() -> usize {
    10000
}

fn default_metric_buffer() -> usize {
    50000
}

fn default_log_buffer() -> usize {
    100000
}

fn default_retention() -> String {
    "1h".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DashboardConfig {
    #[serde(default = "default_dashboard_port")]
    pub port: u16,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub otel: Option<OtelConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OtelConfig {
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_trace_buffer")]
    pub trace_buffer: usize,
    #[serde(default = "default_metric_buffer")]
    pub metric_buffer: usize,
    #[serde(default = "default_log_buffer")]
    pub log_buffer: usize,
    #[serde(default = "default_retention")]
    pub retention: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            port: default_dashboard_port(),
            enabled: None,
            otel: None,
        }
    }
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            grpc_port: default_grpc_port(),
            http_port: default_http_port(),
            trace_buffer: default_trace_buffer(),
            metric_buffer: default_metric_buffer(),
            log_buffer: default_log_buffer(),
            retention: default_retention(),
        }
    }
}

fn default_agents() -> u32 {
    1
}

fn default_dockerfile() -> String {
    "Dockerfile".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_agents")]
    pub agents: u32,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub registry: bool,
    #[serde(default, rename = "image")]
    pub images: BTreeMap<String, ClusterImageConfig>,
    #[serde(default)]
    pub deploy: BTreeMap<String, ClusterDeployConfig>,
    #[serde(default)]
    pub addons: BTreeMap<String, AddonConfig>,
    #[serde(default)]
    pub logs: Option<ClusterLogsConfig>,
    #[serde(default)]
    pub registries: Vec<ClusterRegistryAuth>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ClusterRegistryAuth {
    pub url: String,
    pub username: String,
    pub password: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterLogsConfig {
    /// Enable log collection from the cluster. Default: true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Deploy built-in Fluent Bit collector. Set false if bringing your own.
    #[serde(default = "default_true")]
    pub collector: bool,
    /// Which namespaces to collect logs from. Default: ["default"].
    #[serde(default)]
    pub namespaces: NamespaceFilter,
    /// Namespaces to exclude (only valid when namespaces = "all").
    #[serde(default)]
    pub exclude_namespaces: Option<Vec<String>>,
    /// Pod name patterns to exclude from log collection.
    #[serde(default)]
    pub exclude_pods: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub enum NamespaceFilter {
    All,
    List(Vec<String>),
}

impl Default for NamespaceFilter {
    fn default() -> Self {
        NamespaceFilter::List(vec!["default".to_string()])
    }
}

impl<'de> Deserialize<'de> for NamespaceFilter {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NamespaceFilterVisitor;

        impl<'de> de::Visitor<'de> for NamespaceFilterVisitor {
            type Value = NamespaceFilter;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "the string \"all\" or a list of namespace strings")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "all" {
                    Ok(NamespaceFilter::All)
                } else {
                    Err(E::custom(format!("expected \"all\" but got \"{}\"", v)))
                }
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut names = Vec::new();
                while let Some(name) = seq.next_element::<String>()? {
                    names.push(name);
                }
                Ok(NamespaceFilter::List(names))
            }
        }

        deserializer.deserialize_any(NamespaceFilterVisitor)
    }
}

fn default_helm_timeout() -> String {
    "5m".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum AddonConfig {
    #[serde(rename = "helm")]
    Helm {
        chart: String,
        #[serde(default)]
        repo: Option<String>,
        namespace: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        values: BTreeMap<String, toml::Value>,
        #[serde(default)]
        values_files: Vec<String>,
        #[serde(default)]
        port_forward: BTreeMap<String, String>,
        #[serde(default = "default_true")]
        wait: bool,
        #[serde(default = "default_helm_timeout")]
        timeout: String,
        #[serde(default)]
        depends_on: Vec<String>,
    },
    #[serde(rename = "manifest")]
    Manifest {
        path: String,
        #[serde(default)]
        namespace: Option<String>,
        #[serde(default)]
        port_forward: BTreeMap<String, String>,
        #[serde(default)]
        depends_on: Vec<String>,
    },
    #[serde(rename = "kustomize")]
    Kustomize {
        path: String,
        #[serde(default)]
        namespace: Option<String>,
        #[serde(default)]
        port_forward: BTreeMap<String, String>,
        #[serde(default)]
        depends_on: Vec<String>,
    },
}

impl AddonConfig {
    /// Returns the raw port_forward map for any addon variant.
    pub fn port_forward(&self) -> &BTreeMap<String, String> {
        match self {
            AddonConfig::Helm { port_forward, .. } => port_forward,
            AddonConfig::Manifest { port_forward, .. } => port_forward,
            AddonConfig::Kustomize { port_forward, .. } => port_forward,
        }
    }

    /// Returns parsed port_forward entries as (local_port, target) pairs.
    pub fn parsed_port_forwards(&self) -> Vec<(u16, String)> {
        self.port_forward()
            .iter()
            .filter_map(|(k, v)| k.parse::<u16>().ok().map(|port| (port, v.clone())))
            .collect()
    }

    /// Returns the namespace for any addon variant.
    pub fn namespace(&self) -> Option<&str> {
        match self {
            AddonConfig::Helm { namespace, .. } => Some(namespace.as_str()),
            AddonConfig::Manifest { namespace, .. } => namespace.as_deref(),
            AddonConfig::Kustomize { namespace, .. } => namespace.as_deref(),
        }
    }

    /// Returns the addon type as a string.
    pub fn addon_type(&self) -> &str {
        match self {
            AddonConfig::Helm { .. } => "helm",
            AddonConfig::Manifest { .. } => "manifest",
            AddonConfig::Kustomize { .. } => "kustomize",
        }
    }

    /// Returns the depends_on list for any addon variant.
    pub fn depends_on(&self) -> &[String] {
        match self {
            AddonConfig::Helm { depends_on, .. } => depends_on,
            AddonConfig::Manifest { depends_on, .. } => depends_on,
            AddonConfig::Kustomize { depends_on, .. } => depends_on,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterImageConfig {
    pub context: String,
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,
    #[serde(default)]
    pub watch: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterDeployConfig {
    pub context: String,
    #[serde(default = "default_dockerfile")]
    pub dockerfile: String,
    pub manifests: String,
    #[serde(default)]
    pub watch: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
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

    // --- v0.2 DockerConfig tests ---

    #[test]
    fn parse_infra_single_port() {
        let toml = r#"
            [project]
            name = "test"

            [docker.postgres]
            image = "postgres:16-alpine"
            port = 5432
            [docker.postgres.env]
            POSTGRES_USER = "devrig"
            POSTGRES_PASSWORD = "devrig"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.docker.len(), 1);
        let pg = &config.docker["postgres"];
        assert_eq!(pg.image, "postgres:16-alpine");
        assert!(matches!(pg.port, Some(Port::Fixed(5432))));
        assert_eq!(pg.env["POSTGRES_USER"], "devrig");
    }

    #[test]
    fn parse_docker_named_ports() {
        let toml = r#"
            [project]
            name = "test"

            [docker.mailpit]
            image = "axllent/mailpit:latest"
            [docker.mailpit.ports]
            smtp = 1025
            ui = 8025
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let mp = &config.docker["mailpit"];
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
            [docker.redis]
            image = "redis:7-alpine"
            port = "auto"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(config.docker["redis"].port, Some(Port::Auto)));
    }

    #[test]
    fn parse_ready_check_pg_isready() {
        let toml = r#"
            [project]
            name = "test"
            [docker.postgres]
            image = "postgres:16"
            port = 5432
            ready_check = { type = "pg_isready" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.docker["postgres"].ready_check,
            Some(ReadyCheck::PgIsReady { .. })
        ));
    }

    #[test]
    fn parse_ready_check_cmd() {
        let toml = r#"
            [project]
            name = "test"
            [docker.redis]
            image = "redis:7"
            port = 6379
            [docker.redis.ready_check]
            type = "cmd"
            command = "redis-cli ping"
            expect = "PONG"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.docker["redis"].ready_check {
            Some(ReadyCheck::Cmd { command, expect, .. }) => {
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
            [docker.minio]
            image = "minio/minio"
            port = 9000
            ready_check = { type = "http", url = "http://localhost:9000/minio/health/live" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.docker["minio"].ready_check {
            Some(ReadyCheck::Http { url, .. }) => {
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
            [docker.redis]
            image = "redis:7"
            port = 6379
            ready_check = { type = "tcp" }
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(matches!(
            config.docker["redis"].ready_check,
            Some(ReadyCheck::Tcp { .. })
        ));
    }

    #[test]
    fn parse_ready_check_log() {
        let toml = r#"
            [project]
            name = "test"
            [docker.postgres]
            image = "postgres:16"
            port = 5432
            [docker.postgres.ready_check]
            type = "log"
            match = "ready to accept connections"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        match &config.docker["postgres"].ready_check {
            Some(ReadyCheck::Log { pattern, .. }) => {
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

            [docker.postgres]
            image = "postgres:16-alpine"
            port = 5432
            [docker.postgres.env]
            POSTGRES_USER = "app"
            POSTGRES_PASSWORD = "secret"

            [docker.redis]
            image = "redis:7-alpine"
            port = 6379

            [services.api]
            command = "cargo run"
            port = 3000
            depends_on = ["postgres"]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.docker.len(), 2);
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.services["api"].depends_on, vec!["postgres"]);
    }

    #[test]
    fn parse_minimal_config_still_works() {
        // Backwards compatibility: v0.1 config with no docker/compose still works
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.docker.is_empty());
        assert!(config.compose.is_none());
        assert!(config.network.is_none());
    }

    #[test]
    fn parse_infra_with_all_fields() {
        let toml = r#"
            [project]
            name = "test"

            [docker.postgres]
            image = "postgres:16-alpine"
            port = 5432
            volumes = ["pgdata:/var/lib/postgresql/data"]
            init = [
                "CREATE DATABASE myapp;",
                "CREATE USER appuser WITH PASSWORD 'secret';",
            ]
            depends_on = ["redis"]

            [docker.postgres.env]
            POSTGRES_USER = "devrig"
            POSTGRES_PASSWORD = "devrig"

            [docker.postgres.ready_check]
            type = "pg_isready"

            [docker.redis]
            image = "redis:7-alpine"
            port = 6379
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let pg = &config.docker["postgres"];
        assert_eq!(pg.image, "postgres:16-alpine");
        assert!(matches!(pg.port, Some(Port::Fixed(5432))));
        assert_eq!(pg.volumes, vec!["pgdata:/var/lib/postgresql/data"]);
        assert_eq!(pg.init.len(), 2);
        assert_eq!(pg.depends_on, vec!["redis"]);
        assert!(matches!(pg.ready_check, Some(ReadyCheck::PgIsReady { .. })));
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

    // --- v0.3 Cluster config tests ---

    #[test]
    fn parse_cluster_with_registry_and_deploy() {
        let toml = r#"
            [project]
            name = "myapp"

            [cluster]
            registry = true
            agents = 2
            ports = ["8080:80@loadbalancer"]

            [cluster.deploy.api]
            context = "./api"
            manifests = "./k8s/api"
            watch = true
            depends_on = ["postgres"]

            [cluster.deploy.worker]
            context = "./worker"
            dockerfile = "Dockerfile.worker"
            manifests = "./k8s/worker"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert!(cluster.registry);
        assert_eq!(cluster.agents, 2);
        assert_eq!(cluster.ports, vec!["8080:80@loadbalancer"]);
        assert_eq!(cluster.deploy.len(), 2);

        let api = &cluster.deploy["api"];
        assert_eq!(api.context, "./api");
        assert_eq!(api.manifests, "./k8s/api");
        assert!(api.watch);
        assert_eq!(api.depends_on, vec!["postgres"]);
        assert_eq!(api.dockerfile, "Dockerfile");

        let worker = &cluster.deploy["worker"];
        assert_eq!(worker.context, "./worker");
        assert_eq!(worker.dockerfile, "Dockerfile.worker");
        assert_eq!(worker.manifests, "./k8s/worker");
        assert!(!worker.watch);
        assert!(worker.depends_on.is_empty());
    }

    #[test]
    fn parse_minimal_cluster_block() {
        let toml = r#"
            [project]
            name = "test"

            [cluster]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert!(cluster.name.is_none());
        assert_eq!(cluster.agents, 1);
        assert!(cluster.ports.is_empty());
        assert!(!cluster.registry);
        assert!(cluster.deploy.is_empty());
    }

    #[test]
    fn parse_cluster_deploy_with_all_fields() {
        let toml = r#"
            [project]
            name = "test"

            [cluster]
            name = "my-cluster"
            registry = true

            [cluster.deploy.svc]
            context = "./src"
            dockerfile = "Dockerfile.prod"
            manifests = "./deploy"
            watch = true
            depends_on = ["redis", "postgres"]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.name.as_deref(), Some("my-cluster"));

        let svc = &cluster.deploy["svc"];
        assert_eq!(svc.context, "./src");
        assert_eq!(svc.dockerfile, "Dockerfile.prod");
        assert_eq!(svc.manifests, "./deploy");
        assert!(svc.watch);
        assert_eq!(svc.depends_on, vec!["redis", "postgres"]);
    }

    #[test]
    fn parse_cluster_deploy_with_defaults() {
        let toml = r#"
            [project]
            name = "test"

            [cluster.deploy.api]
            context = "./api"
            manifests = "./k8s"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        let api = &cluster.deploy["api"];
        assert_eq!(api.dockerfile, "Dockerfile");
        assert!(!api.watch);
        assert!(api.depends_on.is_empty());
    }

    #[test]
    fn parse_config_with_cluster_infra_and_services() {
        let toml = r#"
            [project]
            name = "fullstack"

            [docker.postgres]
            image = "postgres:16-alpine"
            port = 5432

            [cluster]
            registry = true

            [cluster.deploy.api]
            context = "./api"
            manifests = "./k8s/api"
            depends_on = ["postgres"]

            [services.web]
            command = "npm run dev"
            port = 3000
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.docker.len(), 1);
        assert!(config.cluster.is_some());
        assert_eq!(config.cluster.as_ref().unwrap().deploy.len(), 1);
        assert_eq!(config.services.len(), 1);
    }

    #[test]
    fn parse_cluster_image_config() {
        let toml = r#"
            [project]
            name = "myapp"

            [cluster]
            registry = true

            [cluster.image.job-runner]
            context = "./tools/job-runner"
            watch = true

            [cluster.image.migrator]
            context = "./tools/migrator"
            dockerfile = "Dockerfile.migrate"
            depends_on = ["postgres"]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.images.len(), 2);

        let runner = &cluster.images["job-runner"];
        assert_eq!(runner.context, "./tools/job-runner");
        assert_eq!(runner.dockerfile, "Dockerfile");
        assert!(runner.watch);
        assert!(runner.depends_on.is_empty());

        let migrator = &cluster.images["migrator"];
        assert_eq!(migrator.context, "./tools/migrator");
        assert_eq!(migrator.dockerfile, "Dockerfile.migrate");
        assert!(!migrator.watch);
        assert_eq!(migrator.depends_on, vec!["postgres"]);
    }

    #[test]
    fn parse_cluster_without_images() {
        let toml = r#"
            [project]
            name = "test"

            [cluster]
            registry = true
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert!(cluster.images.is_empty());
    }

    #[test]
    fn parse_cluster_image_with_defaults() {
        let toml = r#"
            [project]
            name = "test"

            [cluster.image.builder]
            context = "./builder"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        let builder = &cluster.images["builder"];
        assert_eq!(builder.dockerfile, "Dockerfile");
        assert!(!builder.watch);
        assert!(builder.depends_on.is_empty());
    }

    #[test]
    fn parse_cluster_with_images_and_deploys() {
        let toml = r#"
            [project]
            name = "test"

            [cluster]
            registry = true

            [cluster.image.job-runner]
            context = "./tools/job-runner"

            [cluster.deploy.api]
            context = "./api"
            manifests = "./k8s/api"
            depends_on = ["job-runner"]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.images.len(), 1);
        assert_eq!(cluster.deploy.len(), 1);
        assert_eq!(cluster.deploy["api"].depends_on, vec!["job-runner"]);
    }

    #[test]
    fn parse_minimal_config_without_cluster_still_works() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.cluster.is_none());
    }

    // --- v0.4 RestartConfig tests ---

    #[test]
    fn parse_restart_config_all_fields() {
        let toml = r#"
            [project]
            name = "test"

            [services.api]
            command = "cargo run"
            port = 3000

            [services.api.restart]
            policy = "always"
            max_restarts = 5
            startup_max_restarts = 2
            startup_grace_ms = 3000
            initial_delay_ms = 1000
            max_delay_ms = 60000
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let restart = config.services["api"].restart.as_ref().unwrap();
        assert_eq!(restart.policy, "always");
        assert_eq!(restart.max_restarts, 5);
        assert_eq!(restart.startup_max_restarts, 2);
        assert_eq!(restart.startup_grace_ms, 3000);
        assert_eq!(restart.initial_delay_ms, 1000);
        assert_eq!(restart.max_delay_ms, 60000);
    }

    #[test]
    fn parse_restart_config_defaults() {
        let toml = r#"
            [project]
            name = "test"

            [services.api]
            command = "cargo run"

            [services.api.restart]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let restart = config.services["api"].restart.as_ref().unwrap();
        assert_eq!(restart.policy, "on-failure");
        assert_eq!(restart.max_restarts, 10);
        assert_eq!(restart.startup_max_restarts, 3);
        assert_eq!(restart.startup_grace_ms, 2000);
        assert_eq!(restart.initial_delay_ms, 500);
        assert_eq!(restart.max_delay_ms, 30000);
    }

    #[test]
    fn parse_restart_config_absent() {
        let toml = r#"
            [project]
            name = "test"

            [services.api]
            command = "cargo run"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.services["api"].restart.is_none());
    }

    #[test]
    fn service_config_partial_eq() {
        let a = ServiceConfig {
            path: None,
            command: "echo hi".to_string(),
            port: Some(Port::Fixed(3000)),
            env: BTreeMap::new(),
            env_file: None,
            depends_on: vec![],
            restart: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- v0.5 DashboardConfig tests ---

    #[test]
    fn parse_full_dashboard_config() {
        let toml = r#"
            [project]
            name = "test"

            [dashboard]
            port = 5000
            enabled = true

            [dashboard.otel]
            grpc_port = 14317
            http_port = 14318
            trace_buffer = 5000
            metric_buffer = 25000
            log_buffer = 50000
            retention = "30m"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let dash = config.dashboard.unwrap();
        assert_eq!(dash.port, 5000);
        assert_eq!(dash.enabled, Some(true));
        let otel = dash.otel.unwrap();
        assert_eq!(otel.grpc_port, 14317);
        assert_eq!(otel.http_port, 14318);
        assert_eq!(otel.trace_buffer, 5000);
        assert_eq!(otel.metric_buffer, 25000);
        assert_eq!(otel.log_buffer, 50000);
        assert_eq!(otel.retention, "30m");
    }

    #[test]
    fn parse_minimal_dashboard_port_only() {
        let toml = r#"
            [project]
            name = "test"

            [dashboard]
            port = 9000
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let dash = config.dashboard.unwrap();
        assert_eq!(dash.port, 9000);
        assert!(dash.enabled.is_none());
        assert!(dash.otel.is_none());
    }

    #[test]
    fn parse_dashboard_with_otel_subsection() {
        let toml = r#"
            [project]
            name = "test"

            [dashboard]

            [dashboard.otel]
            trace_buffer = 20000
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let dash = config.dashboard.unwrap();
        assert_eq!(dash.port, 4000); // default
        let otel = dash.otel.unwrap();
        assert_eq!(otel.grpc_port, 4317); // default
        assert_eq!(otel.http_port, 4318); // default
        assert_eq!(otel.trace_buffer, 20000);
        assert_eq!(otel.metric_buffer, 50000); // default
        assert_eq!(otel.log_buffer, 100000); // default
        assert_eq!(otel.retention, "1h"); // default
    }

    #[test]
    fn parse_empty_dashboard_all_defaults() {
        let toml = r#"
            [project]
            name = "test"

            [dashboard]
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        let dash = config.dashboard.unwrap();
        assert_eq!(dash.port, 4000);
        assert!(dash.enabled.is_none());
        assert!(dash.otel.is_none());
    }

    #[test]
    fn existing_config_without_dashboard_still_parses() {
        let toml = r#"
            [project]
            name = "test"
            [services.api]
            command = "echo hi"
        "#;
        let config: DevrigConfig = toml::from_str(toml).unwrap();
        assert!(config.dashboard.is_none());
    }

    // --- v0.6 AddonConfig tests ---

    #[test]
    fn parse_addon_helm_config() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true

            [cluster.addons.traefik]
            type = "helm"
            chart = "traefik/traefik"
            repo = "https://traefik.github.io/charts"
            namespace = "traefik"
            version = "26.0.0"
            port_forward = { 9000 = "svc/traefik:9000" }

            [cluster.addons.traefik.values]
            "ports.web.nodePort" = 32080
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.addons.len(), 1);
        match &cluster.addons["traefik"] {
            AddonConfig::Helm {
                chart,
                repo,
                namespace,
                version,
                values,
                port_forward,
                ..
            } => {
                assert_eq!(chart, "traefik/traefik");
                assert_eq!(repo.as_deref(), Some("https://traefik.github.io/charts"));
                assert_eq!(namespace, "traefik");
                assert_eq!(version.as_deref(), Some("26.0.0"));
                assert_eq!(values.len(), 1);
                assert_eq!(port_forward.len(), 1);
                assert_eq!(port_forward["9000"], "svc/traefik:9000");
            }
            other => panic!("expected Helm addon, got {:?}", other),
        }
    }

    #[test]
    fn parse_addon_manifest_config() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.my-tool]
            type = "manifest"
            path = "./k8s/addons/my-tool.yaml"
            namespace = "tools"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["my-tool"] {
            AddonConfig::Manifest {
                path, namespace, ..
            } => {
                assert_eq!(path, "./k8s/addons/my-tool.yaml");
                assert_eq!(namespace.as_deref(), Some("tools"));
            }
            other => panic!("expected Manifest addon, got {:?}", other),
        }
    }

    #[test]
    fn parse_addon_kustomize_config() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.overlay]
            type = "kustomize"
            path = "./k8s/overlays/dev"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["overlay"] {
            AddonConfig::Kustomize { path, .. } => {
                assert_eq!(path, "./k8s/overlays/dev");
            }
            other => panic!("expected Kustomize addon, got {:?}", other),
        }
    }

    #[test]
    fn parse_cluster_without_addons() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        assert!(cluster.addons.is_empty());
    }

    #[test]
    fn parse_addon_port_forward_map() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.traefik]
            type = "helm"
            chart = "traefik/traefik"
            repo = "https://traefik.github.io/charts"
            namespace = "traefik"
            port_forward = { 9000 = "svc/traefik:9000", 8080 = "svc/traefik:80" }
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        let pf = cluster.addons["traefik"].port_forward();
        assert_eq!(pf.len(), 2);
        assert_eq!(pf["9000"], "svc/traefik:9000");
        assert_eq!(pf["8080"], "svc/traefik:80");
    }

    #[test]
    fn parse_addon_with_values() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.cert-manager]
            type = "helm"
            chart = "jetstack/cert-manager"
            repo = "https://charts.jetstack.io"
            namespace = "cert-manager"

            [cluster.addons.cert-manager.values]
            installCRDs = true
            replicaCount = 2
            webhook_port = 10250
            image_tag = "v1.14.0"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["cert-manager"] {
            AddonConfig::Helm { values, .. } => {
                assert_eq!(values.len(), 4);
                assert_eq!(values["installCRDs"], toml::Value::Boolean(true));
                assert_eq!(values["replicaCount"], toml::Value::Integer(2));
                assert_eq!(
                    values["image_tag"],
                    toml::Value::String("v1.14.0".to_string())
                );
            }
            other => panic!("expected Helm addon, got {:?}", other),
        }
    }

    #[test]
    fn addon_config_helper_methods() {
        let helm = AddonConfig::Helm {
            chart: "test".to_string(),
            repo: Some("https://example.com".to_string()),
            namespace: "default".to_string(),
            version: None,
            values: BTreeMap::new(),
            values_files: Vec::new(),
            port_forward: BTreeMap::from([("8080".to_string(), "svc/test:80".to_string())]),
            wait: true,
            timeout: "5m".to_string(),
            depends_on: vec![],
        };
        assert_eq!(helm.addon_type(), "helm");
        assert_eq!(helm.namespace(), Some("default"));
        assert_eq!(helm.port_forward().len(), 1);
        assert!(helm.depends_on().is_empty());

        let manifest = AddonConfig::Manifest {
            path: "./test.yaml".to_string(),
            namespace: None,
            port_forward: BTreeMap::new(),
            depends_on: vec![],
        };
        assert_eq!(manifest.addon_type(), "manifest");
        assert_eq!(manifest.namespace(), None);
        assert!(manifest.port_forward().is_empty());
        assert!(manifest.depends_on().is_empty());
    }

    #[test]
    fn parse_addon_local_helm_no_repo() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.myapp]
            type = "helm"
            chart = "./charts/myapp"
            namespace = "myapp"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["myapp"] {
            AddonConfig::Helm {
                chart,
                repo,
                namespace,
                values_files,
                ..
            } => {
                assert_eq!(chart, "./charts/myapp");
                assert!(repo.is_none());
                assert_eq!(namespace, "myapp");
                assert!(values_files.is_empty());
            }
            other => panic!("expected Helm addon, got {:?}", other),
        }
    }

    #[test]
    fn parse_addon_local_helm_with_values_files() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.myapp]
            type = "helm"
            chart = "./charts/myapp"
            namespace = "myapp"
            values_files = ["charts/myapp/values-dev.yaml", "charts/myapp/values-local.yaml"]

            [cluster.addons.myapp.values]
            "image.tag" = "dev"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["myapp"] {
            AddonConfig::Helm {
                chart,
                repo,
                values,
                values_files,
                ..
            } => {
                assert_eq!(chart, "./charts/myapp");
                assert!(repo.is_none());
                assert_eq!(values_files.len(), 2);
                assert_eq!(values_files[0], "charts/myapp/values-dev.yaml");
                assert_eq!(values_files[1], "charts/myapp/values-local.yaml");
                assert!(values.contains_key("image.tag"));
            }
            other => panic!("expected Helm addon, got {:?}", other),
        }
    }

    #[test]
    fn parse_addon_remote_helm_with_values_files() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.addons.traefik]
            type = "helm"
            chart = "traefik/traefik"
            repo = "https://traefik.github.io/charts"
            namespace = "traefik"
            values_files = ["helm/traefik-values.yaml"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        match &cluster.addons["traefik"] {
            AddonConfig::Helm {
                repo,
                values_files,
                ..
            } => {
                assert_eq!(repo.as_deref(), Some("https://traefik.github.io/charts"));
                assert_eq!(values_files.len(), 1);
                assert_eq!(values_files[0], "helm/traefik-values.yaml");
            }
            other => panic!("expected Helm addon, got {:?}", other),
        }
    }

    #[test]
    fn dashboard_config_partial_eq() {
        let a = DashboardConfig {
            port: 4000,
            enabled: Some(true),
            otel: Some(OtelConfig::default()),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- ClusterLogsConfig tests ---

    #[test]
    fn parse_minimal_cluster_logs() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true

            [cluster.logs]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        assert!(logs.enabled);
        assert!(logs.collector);
        assert!(matches!(logs.namespaces, NamespaceFilter::List(ref ns) if ns == &["default"]));
        assert!(logs.exclude_namespaces.is_none());
        assert!(logs.exclude_pods.is_none());
    }

    #[test]
    fn parse_cluster_logs_specific_namespaces() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            namespaces = ["default", "my-app"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        match logs.namespaces {
            NamespaceFilter::List(ns) => assert_eq!(ns, vec!["default", "my-app"]),
            _ => panic!("expected NamespaceFilter::List"),
        }
    }

    #[test]
    fn parse_cluster_logs_all_namespaces() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            namespaces = "all"
            exclude_namespaces = ["kube-system", "traefik"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        assert!(matches!(logs.namespaces, NamespaceFilter::All));
        assert_eq!(
            logs.exclude_namespaces.unwrap(),
            vec!["kube-system", "traefik"]
        );
    }

    #[test]
    fn parse_cluster_logs_byo_collector() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            collector = false
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        assert!(logs.enabled);
        assert!(!logs.collector);
    }

    #[test]
    fn parse_cluster_logs_disabled() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            enabled = false
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        assert!(!logs.enabled);
    }

    #[test]
    fn parse_cluster_logs_with_exclude_pods() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            namespaces = "all"
            exclude_pods = ["noisy-sidecar-.*", "debug-pod"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let logs = config.cluster.unwrap().logs.unwrap();
        assert_eq!(
            logs.exclude_pods.unwrap(),
            vec!["noisy-sidecar-.*", "debug-pod"]
        );
    }

    #[test]
    fn parse_cluster_without_logs_section() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert!(config.cluster.unwrap().logs.is_none());
    }

    #[test]
    fn namespace_filter_invalid_string_errors() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster.logs]
            namespaces = "invalid"
        "#;
        let err = toml::from_str::<DevrigConfig>(toml_str).unwrap_err();
        assert!(err.to_string().contains("expected \"all\""));
    }

    // --- Secrets management config tests ---

    #[test]
    fn parse_project_env_file() {
        let toml_str = r#"
            [project]
            name = "test"
            env_file = ".env"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.env_file.as_deref(), Some(".env"));
    }

    #[test]
    fn parse_project_without_env_file() {
        let toml_str = r#"
            [project]
            name = "test"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert!(config.project.env_file.is_none());
    }

    #[test]
    fn parse_service_env_file() {
        let toml_str = r#"
            [project]
            name = "test"

            [services.api]
            command = "cargo run"
            env_file = ".env.api"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.services["api"].env_file.as_deref(), Some(".env.api"));
    }

    #[test]
    fn parse_docker_registry_auth() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.my-app]
            image = "ghcr.io/org/app:latest"
            registry_auth = { username = "user", password = "token" }
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let auth = config.docker["my-app"].registry_auth.as_ref().unwrap();
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "token");
    }

    #[test]
    fn parse_docker_without_registry_auth() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.postgres]
            image = "postgres:16"
            port = 5432
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert!(config.docker["postgres"].registry_auth.is_none());
    }

    #[test]
    fn parse_cluster_registries() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true

            [[cluster.registries]]
            url = "ghcr.io"
            username = "user"
            password = "token"

            [[cluster.registries]]
            url = "docker.io"
            username = "ghuser"
            password = "ghtoken"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        assert_eq!(cluster.registries.len(), 2);
        assert_eq!(cluster.registries[0].url, "ghcr.io");
        assert_eq!(cluster.registries[0].username, "user");
        assert_eq!(cluster.registries[1].url, "docker.io");
    }

    #[test]
    fn parse_cluster_without_registries() {
        let toml_str = r#"
            [project]
            name = "test"

            [cluster]
            registry = true
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cluster = config.cluster.unwrap();
        assert!(cluster.registries.is_empty());
    }

    #[test]
    fn backwards_compat_existing_configs_parse() {
        // Ensure all new fields have serde(default) and don't break old configs
        let toml_str = r#"
            [project]
            name = "myapp"

            [env]
            RUST_LOG = "debug"

            [docker.postgres]
            image = "postgres:16-alpine"
            port = 5432

            [services.api]
            command = "cargo run"
            port = 3000
            depends_on = ["postgres"]

            [cluster]
            registry = true
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert!(config.project.env_file.is_none());
        assert!(config.services["api"].env_file.is_none());
        assert!(config.docker["postgres"].registry_auth.is_none());
        assert!(config.cluster.unwrap().registries.is_empty());
    }

    #[test]
    fn parse_docker_command_string() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.redis]
            image = "redis:7-alpine"
            command = "redis-server --appendonly yes"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cmd = config.docker["redis"].command.as_ref().unwrap();
        assert_eq!(cmd.as_slice(), &["redis-server --appendonly yes"]);
    }

    #[test]
    fn parse_docker_command_list() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.redis]
            image = "redis:7-alpine"
            command = ["redis-server", "--appendonly", "yes"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let cmd = config.docker["redis"].command.as_ref().unwrap();
        assert_eq!(cmd.as_slice(), &["redis-server", "--appendonly", "yes"]);
    }

    #[test]
    fn parse_docker_entrypoint_string() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.app]
            image = "myapp:latest"
            entrypoint = "/entrypoint.sh"
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let ep = config.docker["app"].entrypoint.as_ref().unwrap();
        assert_eq!(ep.as_slice(), &["/entrypoint.sh"]);
    }

    #[test]
    fn parse_docker_entrypoint_list() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.app]
            image = "myapp:latest"
            entrypoint = ["python", "-u"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let ep = config.docker["app"].entrypoint.as_ref().unwrap();
        assert_eq!(ep.as_slice(), &["python", "-u"]);
    }

    #[test]
    fn parse_docker_command_and_entrypoint() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.app]
            image = "python:3.12"
            entrypoint = ["python", "-u"]
            command = ["app.py", "--verbose"]
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        let docker = &config.docker["app"];
        let ep = docker.entrypoint.as_ref().unwrap();
        let cmd = docker.command.as_ref().unwrap();
        assert_eq!(ep.as_slice(), &["python", "-u"]);
        assert_eq!(cmd.as_slice(), &["app.py", "--verbose"]);
    }

    #[test]
    fn parse_docker_without_command_or_entrypoint() {
        let toml_str = r#"
            [project]
            name = "test"

            [docker.redis]
            image = "redis:7-alpine"
            port = 6379
        "#;
        let config: DevrigConfig = toml::from_str(toml_str).unwrap();
        assert!(config.docker["redis"].command.is_none());
        assert!(config.docker["redis"].entrypoint.is_none());
    }

    #[test]
    fn string_or_list_into_vec() {
        let sol = StringOrList(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(sol.into_vec(), vec!["a".to_string(), "b".to_string()]);
    }
}
