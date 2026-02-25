use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use crate::config::model::DevrigConfig;
use crate::orchestrator::state::ClusterDeployState;

/// Minimum Jaro-Winkler score to consider a template variable a close match.
const TEMPLATE_SUGGESTION_THRESHOLD: f64 = 0.8;

/// Compiled pattern matching `{{ path.to.value }}` template expressions.
static TEMPLATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([\w.]+)\s*\}\}").expect("template regex must compile"));

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("unresolved variable '{{{{{{ {variable} }}}}}}' in {field}{}", suggestion.as_ref().map(|s| format!(" (did you mean `{}`?)", s)).unwrap_or_default())]
    UnresolvedVariable {
        field: String,
        variable: String,
        suggestion: Option<String>,
    },
}

/// Find the closest matching template variable name using Jaro-Winkler similarity.
fn find_closest_template_var<'a>(name: &str, vars: &'a HashMap<String, String>) -> Option<&'a str> {
    let mut best: Option<(&str, f64)> = None;
    for key in vars.keys() {
        let score = strsim::jaro_winkler(name, key);
        if score >= TEMPLATE_SUGGESTION_THRESHOLD && best.is_none_or(|(_, s)| score > s) {
            best = Some((key.as_str(), score));
        }
    }
    best.map(|(name, _)| name)
}

/// Resolve all `{{ var }}` expressions in `input` using `vars`.
///
/// Two-pass approach:
///   1. Validate that every referenced variable exists in `vars`.
///   2. Replace all references with their values.
///
/// Returns `Ok(resolved_string)` or `Err(vec_of_errors)` listing every
/// unresolved variable.
pub fn resolve_template(
    input: &str,
    vars: &HashMap<String, String>,
    field_context: &str,
) -> Result<String, Vec<TemplateError>> {
    // Pass 1: collect all unresolved references
    let errors: Vec<TemplateError> = TEMPLATE_RE
        .captures_iter(input)
        .filter_map(|cap| {
            let variable = cap[1].to_string();
            if vars.contains_key(&variable) {
                None
            } else {
                let suggestion = find_closest_template_var(&variable, vars).map(String::from);
                Some(TemplateError::UnresolvedVariable {
                    field: field_context.to_string(),
                    variable,
                    suggestion,
                })
            }
        })
        .collect();

    if !errors.is_empty() {
        return Err(errors);
    }

    // Pass 2: replace all references
    let resolved = TEMPLATE_RE
        .replace_all(input, |cap: &regex::Captures| {
            let key = &cap[1];
            vars.get(key).cloned().unwrap_or_default()
        })
        .into_owned();

    Ok(resolved)
}

/// Build the lookup table from a fully-parsed config and a map of resolved
/// ports.
///
/// Produced keys:
///   - `project.name`
///   - `services.{name}.port`       (from resolved_ports key `"service:{name}"`)
///   - `docker.{name}.port`          (from resolved_ports key `"docker:{name}"`)
///   - `docker.{name}.ports.{pname}` (from resolved_ports key `"docker:{name}:{pname}"`)
pub fn build_template_vars(
    config: &DevrigConfig,
    resolved_ports: &HashMap<String, u16>,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // project.name
    vars.insert("project.name".to_string(), config.project.name.clone());

    // services.{name}.port
    for name in config.services.keys() {
        let port_key = format!("service:{name}");
        if let Some(&port) = resolved_ports.get(&port_key) {
            vars.insert(format!("services.{name}.port"), port.to_string());
        }
    }

    // docker.{name}.port and docker.{name}.ports.{pname}
    for (name, docker_cfg) in &config.docker {
        // Single port
        let port_key = format!("docker:{name}");
        if let Some(&port) = resolved_ports.get(&port_key) {
            vars.insert(format!("docker.{name}.port"), port.to_string());
        }

        // Named ports (canonical + short alias)
        for pname in docker_cfg.ports.keys() {
            let port_key = format!("docker:{name}:{pname}");
            if let Some(&port) = resolved_ports.get(&port_key) {
                let val = port.to_string();
                vars.insert(format!("docker.{name}.ports.{pname}"), val.clone());
                vars.insert(format!("docker.{name}.port_{pname}"), val);
            }
        }
    }

    // cluster.name
    if let Some(cluster) = &config.cluster {
        let cluster_name = cluster
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-dev", config.project.name));
        vars.insert("cluster.name".to_string(), cluster_name);
    }

    // dashboard.port, dashboard.otel.grpc_port, dashboard.otel.http_port
    if let Some(dashboard) = &config.dashboard {
        vars.insert("dashboard.port".to_string(), dashboard.port.to_string());
        if let Some(otel) = &dashboard.otel {
            vars.insert(
                "dashboard.otel.grpc_port".to_string(),
                otel.grpc_port.to_string(),
            );
            vars.insert(
                "dashboard.otel.http_port".to_string(),
                otel.http_port.to_string(),
            );
        }
    }

    vars
}

/// Build template variables from cluster image build results.
///
/// Produced keys:
///   - `cluster.image.{name}.tag`  (just the tag portion, e.g. `1234567890`)
pub fn build_cluster_image_vars(
    deployed: &BTreeMap<String, ClusterDeployState>,
) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for (name, state) in deployed {
        // image_tag is the full reference: "localhost:5000/name:tag" or "devrig-name:tag".
        // Extract just the tag portion after the last ':'.
        let tag = state
            .image_tag
            .rsplit_once(':')
            .map(|(_, t)| t)
            .unwrap_or(&state.image_tag);
        vars.insert(format!("cluster.image.{name}.tag"), tag.to_string());
    }
    vars
}

/// Walk every service env value and project-level `[env]` value in `config`
/// and resolve template expressions.
///
/// All errors across all fields are collected and returned together.
pub fn resolve_config_templates(
    config: &mut DevrigConfig,
    vars: &HashMap<String, String>,
) -> Result<(), Vec<TemplateError>> {
    let mut all_errors: Vec<TemplateError> = Vec::new();

    // Resolve project-level [env] templates
    for (env_key, env_val) in &mut config.env {
        let field_context = format!("env.{env_key}");
        match resolve_template(env_val, vars, &field_context) {
            Ok(resolved) => *env_val = resolved,
            Err(mut errs) => all_errors.append(&mut errs),
        }
    }

    // Resolve per-service env templates
    for (svc_name, svc) in &mut config.services {
        for (env_key, env_val) in &mut svc.env {
            let field_context = format!("services.{svc_name}.env.{env_key}");
            match resolve_template(env_val, vars, &field_context) {
                Ok(resolved) => *env_val = resolved,
                Err(mut errs) => all_errors.append(&mut errs),
            }
        }
    }

    if all_errors.is_empty() {
        Ok(())
    } else {
        Err(all_errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{DevrigConfig, DockerConfig, Port, ProjectConfig, ServiceConfig};
    use std::collections::BTreeMap;

    fn make_vars() -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("docker.postgres.port".to_string(), "5432".to_string());
        vars.insert("docker.redis.port".to_string(), "6379".to_string());
        vars.insert("project.name".to_string(), "myapp".to_string());
        vars.insert("docker.mailpit.ports.smtp".to_string(), "1025".to_string());
        vars
    }

    #[test]
    fn basic_substitution() {
        let vars = make_vars();
        let result =
            resolve_template("port={{ docker.postgres.port }}", &vars, "test_field").unwrap();
        assert_eq!(result, "port=5432");
    }

    #[test]
    fn multiple_substitutions() {
        let vars = make_vars();
        let input = "pg={{ docker.postgres.port }},redis={{ docker.redis.port }}";
        let result = resolve_template(input, &vars, "test_field").unwrap();
        assert_eq!(result, "pg=5432,redis=6379");
    }

    #[test]
    fn unresolved_variable_error() {
        let vars = make_vars();
        let result = resolve_template(
            "host={{ docker.mysql.port }}",
            &vars,
            "services.api.env.DB_HOST",
        );
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TemplateError::UnresolvedVariable {
                field, variable, ..
            } => {
                assert_eq!(field, "services.api.env.DB_HOST");
                assert_eq!(variable, "docker.mysql.port");
            }
        }
    }

    #[test]
    fn unresolved_variable_suggests_close_match() {
        let vars = make_vars();
        let result = resolve_template(
            "port={{ docker.postgres.prot }}",
            &vars,
            "services.api.env.PORT",
        );
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TemplateError::UnresolvedVariable { suggestion, .. } => {
                assert_eq!(suggestion.as_deref(), Some("docker.postgres.port"));
            }
        }
    }

    #[test]
    fn port_alias_works_in_template() {
        let mut vars = make_vars();
        vars.insert("docker.mailpit.port_smtp".to_string(), "1025".to_string());
        let result =
            resolve_template("smtp={{ docker.mailpit.port_smtp }}", &vars, "test").unwrap();
        assert_eq!(result, "smtp=1025");
    }

    #[test]
    fn no_templates_is_noop() {
        let vars = make_vars();
        let input = "plain string with no templates";
        let result = resolve_template(input, &vars, "test_field").unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn whitespace_in_braces() {
        let vars = make_vars();
        let result =
            resolve_template("port={{  docker.postgres.port  }}", &vars, "test_field").unwrap();
        assert_eq!(result, "port=5432");
    }

    #[test]
    fn build_template_vars_produces_correct_keys() {
        let mut services = BTreeMap::new();
        services.insert(
            "api".to_string(),
            ServiceConfig {
                path: None,
                command: "cargo run".to_string(),
                port: Some(Port::Auto),
                env: BTreeMap::new(),
                env_file: None,
                depends_on: vec![],
                restart: None,
            },
        );

        let mut mailpit_ports = BTreeMap::new();
        mailpit_ports.insert("smtp".to_string(), Port::Fixed(1025));
        mailpit_ports.insert("ui".to_string(), Port::Fixed(8025));

        let mut docker_map = BTreeMap::new();
        docker_map.insert(
            "postgres".to_string(),
            DockerConfig {
                image: "postgres:16".to_string(),
                port: Some(Port::Fixed(5432)),
                container_port: None,
                ports: BTreeMap::new(),
                env: BTreeMap::new(),
                volumes: vec![],
                command: None,
                entrypoint: None,
                ready_check: None,
                init: vec![],
                depends_on: vec![],
                registry_auth: None,
            },
        );
        docker_map.insert(
            "mailpit".to_string(),
            DockerConfig {
                image: "axllent/mailpit:latest".to_string(),
                port: None,
                container_port: None,
                ports: mailpit_ports,
                env: BTreeMap::new(),
                volumes: vec![],
                command: None,
                entrypoint: None,
                ready_check: None,
                init: vec![],
                depends_on: vec![],
                registry_auth: None,
            },
        );

        let config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
                env_file: None,
            },
            services,
            docker: docker_map,
            compose: None,
            cluster: None,
            dashboard: None,
            env: BTreeMap::new(),
            network: None,
        };

        let mut resolved_ports = HashMap::new();
        resolved_ports.insert("service:api".to_string(), 3000u16);
        resolved_ports.insert("docker:postgres".to_string(), 5432u16);
        resolved_ports.insert("docker:mailpit:smtp".to_string(), 1025u16);
        resolved_ports.insert("docker:mailpit:ui".to_string(), 8025u16);

        let vars = build_template_vars(&config, &resolved_ports);

        assert_eq!(vars.get("project.name").unwrap(), "myapp");
        assert_eq!(vars.get("services.api.port").unwrap(), "3000");
        assert_eq!(vars.get("docker.postgres.port").unwrap(), "5432");
        assert_eq!(vars.get("docker.mailpit.ports.smtp").unwrap(), "1025");
        assert_eq!(vars.get("docker.mailpit.ports.ui").unwrap(), "8025");
        // Short aliases
        assert_eq!(vars.get("docker.mailpit.port_smtp").unwrap(), "1025");
        assert_eq!(vars.get("docker.mailpit.port_ui").unwrap(), "8025");
    }

    #[test]
    fn dashboard_template_vars() {
        use crate::config::model::{DashboardConfig, OtelConfig};
        let config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
                env_file: None,
            },
            services: BTreeMap::new(),
            docker: BTreeMap::new(),
            compose: None,
            cluster: None,
            dashboard: Some(DashboardConfig {
                port: 5000,
                enabled: None,
                otel: Some(OtelConfig {
                    grpc_port: 14317,
                    http_port: 14318,
                    ..OtelConfig::default()
                }),
            }),
            env: BTreeMap::new(),
            network: None,
        };

        let resolved_ports = HashMap::new();
        let vars = build_template_vars(&config, &resolved_ports);
        assert_eq!(vars.get("dashboard.port").unwrap(), "5000");
        assert_eq!(vars.get("dashboard.otel.grpc_port").unwrap(), "14317");
        assert_eq!(vars.get("dashboard.otel.http_port").unwrap(), "14318");
    }

    #[test]
    fn cluster_name_template_var() {
        let mut config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
                env_file: None,
            },
            services: BTreeMap::new(),
            docker: BTreeMap::new(),
            compose: None,
            cluster: Some(crate::config::model::ClusterConfig {
                name: Some("my-cluster".to_string()),
                agents: 1,
                ports: vec![],
                volumes: vec![],
                registry: false,
                images: BTreeMap::new(),
                deploy: BTreeMap::new(),
                addons: BTreeMap::new(),
                logs: None,
                registries: vec![],
            }),
            dashboard: None,
            env: BTreeMap::new(),
            network: None,
        };

        let resolved_ports = HashMap::new();
        let vars = build_template_vars(&config, &resolved_ports);
        assert_eq!(vars.get("cluster.name").unwrap(), "my-cluster");

        // Test default name
        config.cluster.as_mut().unwrap().name = None;
        let vars = build_template_vars(&config, &resolved_ports);
        assert_eq!(vars.get("cluster.name").unwrap(), "myapp-dev");
    }

    #[test]
    fn resolve_config_templates_resolves_global_env() {
        let mut config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
                env_file: None,
            },
            services: BTreeMap::new(),
            docker: BTreeMap::new(),
            compose: None,
            cluster: None,
            dashboard: None,
            env: BTreeMap::from([
                (
                    "DATABASE_URL".to_string(),
                    "postgres://localhost:{{ docker.postgres.port }}/mydb".to_string(),
                ),
                ("PLAIN".to_string(), "no-templates-here".to_string()),
            ]),
            network: None,
        };

        let mut vars = HashMap::new();
        vars.insert("docker.postgres.port".to_string(), "5432".to_string());

        resolve_config_templates(&mut config, &vars).unwrap();

        assert_eq!(
            config.env.get("DATABASE_URL").unwrap(),
            "postgres://localhost:5432/mydb"
        );
        assert_eq!(config.env.get("PLAIN").unwrap(), "no-templates-here");
    }
}
