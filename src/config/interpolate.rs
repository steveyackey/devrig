use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::config::model::DevrigConfig;

/// Compiled pattern matching `{{ path.to.value }}` template expressions.
static TEMPLATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([\w.]+)\s*\}\}").expect("template regex must compile"));

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("unresolved variable '{{{{{{ {variable} }}}}}}' in {field}")]
    UnresolvedVariable { field: String, variable: String },
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
                Some(TemplateError::UnresolvedVariable {
                    field: field_context.to_string(),
                    variable,
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
///   - `infra.{name}.port`          (from resolved_ports key `"infra:{name}"`)
///   - `infra.{name}.ports.{pname}` (from resolved_ports key `"infra:{name}:{pname}"`)
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

    // infra.{name}.port and infra.{name}.ports.{pname}
    for (name, infra) in &config.infra {
        // Single port
        let port_key = format!("infra:{name}");
        if let Some(&port) = resolved_ports.get(&port_key) {
            vars.insert(format!("infra.{name}.port"), port.to_string());
        }

        // Named ports
        for pname in infra.ports.keys() {
            let port_key = format!("infra:{name}:{pname}");
            if let Some(&port) = resolved_ports.get(&port_key) {
                vars.insert(format!("infra.{name}.ports.{pname}"), port.to_string());
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

/// Walk every service env value in `config` and resolve template expressions.
///
/// All errors across all fields are collected and returned together.
pub fn resolve_config_templates(
    config: &mut DevrigConfig,
    vars: &HashMap<String, String>,
) -> Result<(), Vec<TemplateError>> {
    let mut all_errors: Vec<TemplateError> = Vec::new();

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
    use crate::config::model::{DevrigConfig, InfraConfig, Port, ProjectConfig, ServiceConfig};
    use std::collections::BTreeMap;

    fn make_vars() -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("infra.postgres.port".to_string(), "5432".to_string());
        vars.insert("infra.redis.port".to_string(), "6379".to_string());
        vars.insert("project.name".to_string(), "myapp".to_string());
        vars.insert("infra.mailpit.ports.smtp".to_string(), "1025".to_string());
        vars
    }

    #[test]
    fn basic_substitution() {
        let vars = make_vars();
        let result =
            resolve_template("port={{ infra.postgres.port }}", &vars, "test_field").unwrap();
        assert_eq!(result, "port=5432");
    }

    #[test]
    fn multiple_substitutions() {
        let vars = make_vars();
        let input = "pg={{ infra.postgres.port }},redis={{ infra.redis.port }}";
        let result = resolve_template(input, &vars, "test_field").unwrap();
        assert_eq!(result, "pg=5432,redis=6379");
    }

    #[test]
    fn unresolved_variable_error() {
        let vars = make_vars();
        let result = resolve_template(
            "host={{ infra.mysql.port }}",
            &vars,
            "services.api.env.DB_HOST",
        );
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TemplateError::UnresolvedVariable { field, variable } => {
                assert_eq!(field, "services.api.env.DB_HOST");
                assert_eq!(variable, "infra.mysql.port");
            }
        }
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
            resolve_template("port={{  infra.postgres.port  }}", &vars, "test_field").unwrap();
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
                depends_on: vec![],
                restart: None,
            },
        );

        let mut mailpit_ports = BTreeMap::new();
        mailpit_ports.insert("smtp".to_string(), Port::Fixed(1025));
        mailpit_ports.insert("ui".to_string(), Port::Fixed(8025));

        let mut infra = BTreeMap::new();
        infra.insert(
            "postgres".to_string(),
            InfraConfig {
                image: "postgres:16".to_string(),
                port: Some(Port::Fixed(5432)),
                ports: BTreeMap::new(),
                env: BTreeMap::new(),
                volumes: vec![],
                ready_check: None,
                init: vec![],
                depends_on: vec![],
            },
        );
        infra.insert(
            "mailpit".to_string(),
            InfraConfig {
                image: "axllent/mailpit:latest".to_string(),
                port: None,
                ports: mailpit_ports,
                env: BTreeMap::new(),
                volumes: vec![],
                ready_check: None,
                init: vec![],
                depends_on: vec![],
            },
        );

        let config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
            },
            services,
            infra,
            compose: None,
            cluster: None,
            dashboard: None,
            env: BTreeMap::new(),
            network: None,
        };

        let mut resolved_ports = HashMap::new();
        resolved_ports.insert("service:api".to_string(), 3000u16);
        resolved_ports.insert("infra:postgres".to_string(), 5432u16);
        resolved_ports.insert("infra:mailpit:smtp".to_string(), 1025u16);
        resolved_ports.insert("infra:mailpit:ui".to_string(), 8025u16);

        let vars = build_template_vars(&config, &resolved_ports);

        assert_eq!(vars.get("project.name").unwrap(), "myapp");
        assert_eq!(vars.get("services.api.port").unwrap(), "3000");
        assert_eq!(vars.get("infra.postgres.port").unwrap(), "5432");
        assert_eq!(vars.get("infra.mailpit.ports.smtp").unwrap(), "1025");
        assert_eq!(vars.get("infra.mailpit.ports.ui").unwrap(), "8025");
    }

    #[test]
    fn dashboard_template_vars() {
        use crate::config::model::{DashboardConfig, OtelConfig};
        let config = DevrigConfig {
            project: ProjectConfig {
                name: "myapp".to_string(),
            },
            services: BTreeMap::new(),
            infra: BTreeMap::new(),
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
            },
            services: BTreeMap::new(),
            infra: BTreeMap::new(),
            compose: None,
            cluster: Some(crate::config::model::ClusterConfig {
                name: Some("my-cluster".to_string()),
                agents: 1,
                ports: vec![],
                registry: false,
                deploy: BTreeMap::new(),
                addons: BTreeMap::new(),
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
}
