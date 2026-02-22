// The miette/thiserror derive macros generate code that triggers false
// positive unused_assignments warnings on enum variant fields.
#![allow(unused_assignments)]

use std::collections::{BTreeMap, HashSet};

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::config::model::{DevrigConfig, Port};

// ---------------------------------------------------------------------------
// ConfigDiagnostic — miette-powered validation error
// ---------------------------------------------------------------------------

#[derive(Debug, Error, Diagnostic)]
pub enum ConfigDiagnostic {
    #[error("unknown dependency `{dependency}`")]
    #[diagnostic(code(devrig::missing_dependency))]
    MissingDependency {
        #[source_code]
        src: NamedSource<String>,
        #[label("service `{service}` depends on `{dependency}`, which does not exist")]
        span: SourceSpan,
        #[help]
        advice: String,
        service: String,
        dependency: String,
    },

    #[error("port {port} is used by multiple resources: {services:?}")]
    #[diagnostic(code(devrig::duplicate_port))]
    DuplicatePort {
        #[source_code]
        src: NamedSource<String>,
        #[label("duplicate port")]
        span: SourceSpan,
        port: u16,
        services: Vec<String>,
    },

    #[error("dependency cycle detected involving `{node}`")]
    #[diagnostic(code(devrig::dependency_cycle))]
    DependencyCycle {
        #[source_code]
        src: NamedSource<String>,
        #[label("cycle involves this resource")]
        span: SourceSpan,
        node: String,
    },

    #[error("service `{service}` has an empty command")]
    #[diagnostic(code(devrig::empty_command))]
    EmptyCommand {
        #[source_code]
        src: NamedSource<String>,
        #[label("command is empty")]
        span: SourceSpan,
        service: String,
    },

    #[error("infra `{service}` has an empty image")]
    #[diagnostic(code(devrig::empty_image))]
    EmptyImage {
        #[source_code]
        src: NamedSource<String>,
        #[label("image is empty")]
        span: SourceSpan,
        service: String,
    },

    #[error("compose.file is empty")]
    #[diagnostic(code(devrig::empty_compose_file))]
    EmptyComposeFile {
        #[source_code]
        src: NamedSource<String>,
        #[label("file path is empty")]
        span: SourceSpan,
    },

    #[error("cluster deploy `{deploy}` has an empty context")]
    #[diagnostic(code(devrig::empty_deploy_context))]
    EmptyDeployContext {
        #[source_code]
        src: NamedSource<String>,
        #[label("context is empty")]
        span: SourceSpan,
        deploy: String,
    },

    #[error("cluster deploy `{deploy}` has an empty manifests path")]
    #[diagnostic(code(devrig::empty_deploy_manifests))]
    EmptyDeployManifests {
        #[source_code]
        src: NamedSource<String>,
        #[label("manifests path is empty")]
        span: SourceSpan,
        deploy: String,
    },

    #[error("resource name `{name}` is used by multiple resource types: {kinds:?}")]
    #[diagnostic(code(devrig::duplicate_resource_name))]
    DuplicateResourceName {
        #[source_code]
        src: NamedSource<String>,
        #[label("name used by {kinds:?}")]
        span: SourceSpan,
        name: String,
        kinds: Vec<String>,
    },

    #[error("invalid restart policy `{value}` for service `{service}`")]
    #[diagnostic(
        code(devrig::invalid_restart_policy),
        help("valid values are: always, on-failure, never")
    )]
    InvalidRestartPolicy {
        #[source_code]
        src: NamedSource<String>,
        #[label("invalid policy")]
        span: SourceSpan,
        service: String,
        value: String,
    },
}

// ---------------------------------------------------------------------------
// Source span helpers
// ---------------------------------------------------------------------------

/// Find the byte offset of a TOML table header like `[services.api]` or `[infra.postgres]`.
fn find_table_span(source: &str, section: &str, name: &str) -> SourceSpan {
    // Try patterns: [section.name], [section.name.something]
    let patterns = [
        format!("[{}.{}]", section, name),
        format!("[{}.{}", section, name), // partial match for nested like [services.api.env]
    ];

    for pat in &patterns {
        if let Some(pos) = source.find(pat) {
            // Find end of the table key (just the name part)
            let name_start = pos + 1 + section.len() + 1; // skip '[', section, '.'
            return (name_start, name.len()).into();
        }
    }

    // Fallback: search for the name as a plain string
    if let Some(pos) = source.find(name) {
        return (pos, name.len()).into();
    }

    (0, 0).into()
}

/// Find the byte offset of a value in a depends_on array for a given service.
fn find_depends_on_value(source: &str, section: &str, service: &str, dep: &str) -> SourceSpan {
    // Look for the depends_on line after the service table header
    let table_header = format!("[{}.{}]", section, service);
    let search_start = source.find(&table_header).unwrap_or(0);

    // Find "dep" in depends_on context after the table header
    let after_header = &source[search_start..];

    // Look for the dependency value in quotes
    let quoted = format!("\"{}\"", dep);
    if let Some(rel_pos) = after_header.find(&quoted) {
        let abs_pos = search_start + rel_pos + 1; // skip the opening quote
        return (abs_pos, dep.len()).into();
    }

    // Fallback: find the dependency name anywhere after the header
    if let Some(rel_pos) = after_header.find(dep) {
        return (search_start + rel_pos, dep.len()).into();
    }

    find_table_span(source, section, service)
}

/// Find the byte offset of a specific field value in a TOML section.
fn find_field_span(source: &str, section: &str, name: &str, field: &str) -> SourceSpan {
    let table_header = format!("[{}.{}]", section, name);
    let search_start = source.find(&table_header).unwrap_or(0);
    let after_header = &source[search_start..];

    // Look for field = "value" or field = value
    let field_prefix = format!("{} =", field);
    let field_prefix2 = format!("{}=", field);

    for prefix in [&field_prefix, &field_prefix2] {
        if let Some(rel_pos) = after_header.find(prefix) {
            let abs_pos = search_start + rel_pos;
            // Find the value part (after the =)
            let eq_pos = source[abs_pos..].find('=').map(|p| abs_pos + p + 1);
            if let Some(val_start) = eq_pos {
                let val_trimmed = source[val_start..].trim_start();
                let trim_offset = val_start + (source[val_start..].len() - val_trimmed.len());
                // Find end of value (newline or end of string)
                let val_end = val_trimmed
                    .find('\n')
                    .unwrap_or(val_trimmed.len())
                    .min(val_trimmed.len());
                return (trim_offset, val_end).into();
            }
        }
    }

    find_table_span(source, section, name)
}

/// Find the byte offset of a port value for a given resource.
fn find_port_span(source: &str, section: &str, name: &str) -> SourceSpan {
    find_field_span(source, section, name, "port")
}

// ---------------------------------------------------------------------------
// Similarity suggestions
// ---------------------------------------------------------------------------

fn find_closest_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let mut best: Option<(&str, f64)> = None;
    for candidate in candidates {
        let score = strsim::jaro_winkler(name, candidate);
        if score >= 0.8 && best.is_none_or(|(_, s)| score > s) {
            best = Some((candidate.as_str(), score));
        }
    }
    best.map(|(name, _)| name)
}

// ---------------------------------------------------------------------------
// Main validation function
// ---------------------------------------------------------------------------

pub fn validate(
    config: &DevrigConfig,
    source: &str,
    filename: &str,
) -> Result<(), Vec<ConfigDiagnostic>> {
    let mut errors = Vec::new();
    let src = NamedSource::new(filename, source.to_string());

    // Build the list of all available names: services + infra + compose.services + cluster.deploy
    let mut available: Vec<String> = config.services.keys().cloned().collect();
    for name in config.infra.keys() {
        available.push(name.clone());
    }
    if let Some(compose) = &config.compose {
        for svc in &compose.services {
            available.push(svc.clone());
        }
    }
    if let Some(cluster) = &config.cluster {
        for name in cluster.deploy.keys() {
            available.push(name.clone());
        }
    }

    // Check all depends_on references exist (services)
    for (name, svc) in &config.services {
        for dep in &svc.depends_on {
            if !available.contains(dep) {
                let suggestion = find_closest_match(dep, &available);
                let advice = match suggestion {
                    Some(s) => format!("did you mean `{}`?", s),
                    None => format!("available resources: {:?}", available),
                };
                errors.push(ConfigDiagnostic::MissingDependency {
                    src: src.clone(),
                    span: find_depends_on_value(source, "services", name, dep),
                    advice,
                    service: name.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Check all depends_on references exist (infra)
    for (name, infra) in &config.infra {
        for dep in &infra.depends_on {
            if !available.contains(dep) {
                let suggestion = find_closest_match(dep, &available);
                let advice = match suggestion {
                    Some(s) => format!("did you mean `{}`?", s),
                    None => format!("available resources: {:?}", available),
                };
                errors.push(ConfigDiagnostic::MissingDependency {
                    src: src.clone(),
                    span: find_depends_on_value(source, "infra", name, dep),
                    advice,
                    service: name.clone(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Check all depends_on references exist (cluster deploy)
    if let Some(cluster) = &config.cluster {
        for (name, deploy) in &cluster.deploy {
            for dep in &deploy.depends_on {
                if !available.contains(dep) {
                    let suggestion = find_closest_match(dep, &available);
                    let advice = match suggestion {
                        Some(s) => format!("did you mean `{}`?", s),
                        None => format!("available resources: {:?}", available),
                    };
                    errors.push(ConfigDiagnostic::MissingDependency {
                        src: src.clone(),
                        span: find_depends_on_value(source, "cluster.deploy", name, dep),
                        advice,
                        service: name.clone(),
                        dependency: dep.clone(),
                    });
                }
            }
        }
    }

    // Check cluster deploy names don't conflict with service, infra, or compose names
    if let Some(cluster) = &config.cluster {
        for name in cluster.deploy.keys() {
            let mut kinds = Vec::new();
            if config.services.contains_key(name) {
                kinds.push("service".to_string());
            }
            if config.infra.contains_key(name) {
                kinds.push("infra".to_string());
            }
            if let Some(compose) = &config.compose {
                if compose.services.contains(name) {
                    kinds.push("compose".to_string());
                }
            }
            if !kinds.is_empty() {
                kinds.push("cluster.deploy".to_string());
                errors.push(ConfigDiagnostic::DuplicateResourceName {
                    src: src.clone(),
                    span: find_table_span(source, "cluster.deploy", name),
                    name: name.clone(),
                    kinds,
                });
            }
        }
    }

    // Check no two services/infra share the same fixed port
    let mut port_map: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for (name, svc) in &config.services {
        if let Some(Port::Fixed(p)) = &svc.port {
            port_map.entry(*p).or_default().push(name.clone());
        }
    }
    for (name, infra) in &config.infra {
        if let Some(Port::Fixed(p)) = &infra.port {
            port_map.entry(*p).or_default().push(name.clone());
        }
        for port_val in infra.ports.values() {
            if let Port::Fixed(p) = port_val {
                port_map.entry(*p).or_default().push(name.clone());
            }
        }
    }
    for (port, services) in port_map {
        if services.len() > 1 {
            // Find span of the first port declaration
            let first = &services[0];
            let section = if config.services.contains_key(first) {
                "services"
            } else {
                "infra"
            };
            errors.push(ConfigDiagnostic::DuplicatePort {
                src: src.clone(),
                span: find_port_span(source, section, first),
                port,
                services,
            });
        }
    }

    // Build a complete deps map from both services and infra for cycle detection
    let mut deps_map: BTreeMap<&str, &Vec<String>> = BTreeMap::new();
    for (name, svc) in &config.services {
        deps_map.insert(name.as_str(), &svc.depends_on);
    }
    for (name, infra) in &config.infra {
        deps_map.insert(name.as_str(), &infra.depends_on);
    }
    if let Some(cluster) = &config.cluster {
        for (name, deploy) in &cluster.deploy {
            deps_map.insert(name.as_str(), &deploy.depends_on);
        }
    }

    // Check for dependency cycles using iterative DFS with visited/in_stack
    {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for start in deps_map.keys() {
            if visited.contains(start) {
                continue;
            }

            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            in_stack.insert(start);

            while let Some((node, idx)) = stack.last_mut() {
                let deps = deps_map[*node];
                if *idx < deps.len() {
                    let dep = deps[*idx].as_str();
                    *idx += 1;

                    if !deps_map.contains_key(dep) {
                        continue;
                    }

                    if in_stack.contains(dep) {
                        // Determine the section for span
                        let section = if config.services.contains_key(dep) {
                            "services"
                        } else if config.infra.contains_key(dep) {
                            "infra"
                        } else {
                            "cluster.deploy"
                        };
                        errors.push(ConfigDiagnostic::DependencyCycle {
                            src: src.clone(),
                            span: find_table_span(source, section, dep),
                            node: dep.to_string(),
                        });
                    } else if !visited.contains(dep) {
                        in_stack.insert(dep);
                        stack.push((dep, 0));
                    }
                } else {
                    let finished = *node;
                    visited.insert(finished);
                    in_stack.remove(finished);
                    stack.pop();
                }
            }
        }
    }

    // Check no service has an empty command string
    for (name, svc) in &config.services {
        if svc.command.trim().is_empty() {
            errors.push(ConfigDiagnostic::EmptyCommand {
                src: src.clone(),
                span: find_field_span(source, "services", name, "command"),
                service: name.clone(),
            });
        }
    }

    // Check no infra entry has an empty image string
    for (name, infra) in &config.infra {
        if infra.image.trim().is_empty() {
            errors.push(ConfigDiagnostic::EmptyImage {
                src: src.clone(),
                span: find_field_span(source, "infra", name, "image"),
                service: name.clone(),
            });
        }
    }

    // Check compose.file is non-empty if compose is present
    if let Some(compose) = &config.compose {
        if compose.file.trim().is_empty() {
            // Find compose.file field
            let span = if let Some(pos) = source.find("[compose]") {
                let after = &source[pos..];
                if let Some(rel) = after.find("file") {
                    (pos + rel, 4).into()
                } else {
                    (pos, 9).into()
                }
            } else {
                (0, 0).into()
            };
            errors.push(ConfigDiagnostic::EmptyComposeFile {
                src: src.clone(),
                span,
            });
        }
    }

    // Check cluster deploy entries have non-empty context and manifests
    if let Some(cluster) = &config.cluster {
        for (name, deploy) in &cluster.deploy {
            if deploy.context.trim().is_empty() {
                errors.push(ConfigDiagnostic::EmptyDeployContext {
                    src: src.clone(),
                    span: find_field_span(source, "cluster.deploy", name, "context"),
                    deploy: name.clone(),
                });
            }
            if deploy.manifests.trim().is_empty() {
                errors.push(ConfigDiagnostic::EmptyDeployManifests {
                    src: src.clone(),
                    span: find_field_span(source, "cluster.deploy", name, "manifests"),
                    deploy: name.clone(),
                });
            }
        }
    }

    // Validate restart config policy values
    for (name, svc) in &config.services {
        if let Some(restart) = &svc.restart {
            let valid_policies = ["always", "on-failure", "never"];
            if !valid_policies.contains(&restart.policy.as_str()) {
                errors.push(ConfigDiagnostic::InvalidRestartPolicy {
                    src: src.clone(),
                    span: find_field_span(source, "services", name, "policy"),
                    service: name.clone(),
                    value: restart.policy.clone(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{
        ClusterConfig, ClusterDeployConfig, ComposeConfig, InfraConfig, ProjectConfig,
        ServiceConfig,
    };

    const TEST_FILENAME: &str = "devrig.toml";

    /// Helper to build a DevrigConfig from a list of service definitions.
    fn make_config(services: Vec<(&str, &str, Option<Port>, Vec<&str>)>) -> DevrigConfig {
        let mut svc_map = BTreeMap::new();
        for (name, command, port, deps) in services {
            svc_map.insert(
                name.to_string(),
                ServiceConfig {
                    path: None,
                    command: command.to_string(),
                    port,
                    env: BTreeMap::new(),
                    depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
                    restart: None,
                },
            );
        }
        DevrigConfig {
            project: ProjectConfig {
                name: "test".to_string(),
            },
            services: svc_map,
            infra: BTreeMap::new(),
            compose: None,
            cluster: None,
            env: BTreeMap::new(),
            network: None,
        }
    }

    /// Helper to build a TOML source for a config (for span tests).
    fn make_source(services: Vec<(&str, &str, Option<Port>, Vec<&str>)>) -> String {
        let mut s = "[project]\nname = \"test\"\n\n".to_string();
        for (name, command, port, deps) in services {
            s.push_str(&format!("[services.{}]\n", name));
            s.push_str(&format!("command = \"{}\"\n", command));
            if let Some(Port::Fixed(p)) = port {
                s.push_str(&format!("port = {}\n", p));
            } else if let Some(Port::Auto) = port {
                s.push_str("port = \"auto\"\n");
            }
            if !deps.is_empty() {
                let dep_strs: Vec<String> = deps.iter().map(|d| format!("\"{}\"", d)).collect();
                s.push_str(&format!("depends_on = [{}]\n", dep_strs.join(", ")));
            }
            s.push('\n');
        }
        s
    }

    /// Helper to build an InfraConfig with minimal fields.
    fn make_infra(image: &str, port: Option<Port>, deps: Vec<&str>) -> InfraConfig {
        InfraConfig {
            image: image.to_string(),
            port,
            ports: BTreeMap::new(),
            env: BTreeMap::new(),
            volumes: Vec::new(),
            ready_check: None,
            init: Vec::new(),
            depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn missing_dependency_detected() {
        let config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["db"],
        )]);
        let source = make_source(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["db"],
        )]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::MissingDependency {
                service,
                dependency,
                ..
            } if service == "api" && dependency == "db"
        ));
    }

    #[test]
    fn missing_dependency_with_suggestion() {
        let mut config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["postres"], // typo for "postgres"
        )]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16", Some(Port::Fixed(5432)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 3000\ndepends_on = [\"postres\"]\n\n[infra.postgres]\nimage = \"postgres:16\"\nport = 5432\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        match &errs[0] {
            ConfigDiagnostic::MissingDependency { advice, .. } => {
                assert!(
                    advice.contains("postgres"),
                    "expected suggestion 'postgres', got: {}",
                    advice
                );
            }
            other => panic!("expected MissingDependency, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_ports_detected() {
        let config = make_config(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec![]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let source = make_source(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec![]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::DuplicatePort { port: 3000, services, .. } if services.len() == 2
        ));
    }

    #[test]
    fn valid_config_passes() {
        let config = make_config(vec![
            (
                "db",
                "docker compose up postgres",
                Some(Port::Fixed(5432)),
                vec![],
            ),
            ("api", "cargo run", Some(Port::Fixed(3000)), vec!["db"]),
            ("web", "npm start", Some(Port::Auto), vec!["api"]),
            ("worker", "cargo run --bin worker", None, vec![]),
        ]);
        let source = make_source(vec![
            (
                "db",
                "docker compose up postgres",
                Some(Port::Fixed(5432)),
                vec![],
            ),
            ("api", "cargo run", Some(Port::Fixed(3000)), vec!["db"]),
            ("web", "npm start", Some(Port::Auto), vec!["api"]),
            ("worker", "cargo run --bin worker", None, vec![]),
        ]);
        assert!(validate(&config, &source, TEST_FILENAME).is_ok());
    }

    #[test]
    fn multiple_errors_collected() {
        let config = make_config(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec!["redis"]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let source = make_source(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec!["redis"]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        assert!(errs.len() >= 2);

        let has_missing_dep = errs
            .iter()
            .any(|e| matches!(e, ConfigDiagnostic::MissingDependency { .. }));
        let has_dup_port = errs
            .iter()
            .any(|e| matches!(e, ConfigDiagnostic::DuplicatePort { .. }));
        assert!(has_missing_dep, "expected a MissingDependency error");
        assert!(has_dup_port, "expected a DuplicatePort error");
    }

    #[test]
    fn empty_command_detected() {
        let config = make_config(vec![("api", "  ", Some(Port::Fixed(3000)), vec![])]);
        let source = make_source(vec![("api", "  ", Some(Port::Fixed(3000)), vec![])]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::EmptyCommand { service, .. } if service == "api"
        ));
    }

    #[test]
    fn self_reference_detected() {
        let config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["api"],
        )]);
        let source = make_source(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["api"],
        )]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        let has_cycle = errs.iter().any(|e| {
            matches!(
                e,
                ConfigDiagnostic::DependencyCycle { node, .. } if node == "api"
            )
        });
        assert!(
            has_cycle,
            "expected a DependencyCycle error for self-reference"
        );
    }

    #[test]
    fn cycle_detected() {
        let config = make_config(vec![
            ("a", "echo a", None, vec!["b"]),
            ("b", "echo b", None, vec!["c"]),
            ("c", "echo c", None, vec!["a"]),
        ]);
        let source = make_source(vec![
            ("a", "echo a", None, vec!["b"]),
            ("b", "echo b", None, vec!["c"]),
            ("c", "echo c", None, vec!["a"]),
        ]);
        let errs = validate(&config, &source, TEST_FILENAME).unwrap_err();
        let has_cycle = errs
            .iter()
            .any(|e| matches!(e, ConfigDiagnostic::DependencyCycle { .. }));
        assert!(has_cycle, "expected a DependencyCycle error for a->b->c->a");
    }

    // --- v0.2 infra/compose validation tests ---

    #[test]
    fn service_depends_on_infra_name_is_valid() {
        let mut config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["postgres"],
        )]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 3000\ndepends_on = [\"postgres\"]\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n";
        assert!(validate(&config, source, TEST_FILENAME).is_ok());
    }

    #[test]
    fn service_depends_on_unknown_name_errors() {
        let mut config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["nonexistent"],
        )]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 3000\ndepends_on = [\"nonexistent\"]\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::MissingDependency {
                service,
                dependency,
                ..
            } if service == "api" && dependency == "nonexistent"
        ));
    }

    #[test]
    fn infra_and_service_share_fixed_port_errors() {
        let mut config = make_config(vec![("api", "cargo run", Some(Port::Fixed(5432)), vec![])]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 5432\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::DuplicatePort { port: 5432, services, .. } if services.len() == 2
        ));
    }

    #[test]
    fn infra_with_empty_image_errors() {
        let mut config = make_config(vec![]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("", Some(Port::Fixed(5432)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[infra.postgres]\nimage = \"\"\nport = 5432\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::EmptyImage { service, .. } if service == "postgres"
        ));
    }

    #[test]
    fn config_with_infra_services_and_cross_type_depends_on_is_valid() {
        let mut config = make_config(vec![
            (
                "api",
                "cargo run",
                Some(Port::Fixed(3000)),
                vec!["postgres", "redis"],
            ),
            ("worker", "cargo run --bin worker", None, vec!["redis"]),
        ]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        config.infra.insert(
            "redis".to_string(),
            make_infra("redis:7-alpine", Some(Port::Fixed(6379)), vec![]),
        );
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 3000\ndepends_on = [\"postgres\", \"redis\"]\n\n[services.worker]\ncommand = \"cargo run --bin worker\"\ndepends_on = [\"redis\"]\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n\n[infra.redis]\nimage = \"redis:7-alpine\"\nport = 6379\n";
        assert!(validate(&config, source, TEST_FILENAME).is_ok());
    }

    #[test]
    fn compose_with_empty_file_errors() {
        let mut config = make_config(vec![]);
        config.compose = Some(ComposeConfig {
            file: "".to_string(),
            services: vec![],
            env_file: None,
            ready_checks: BTreeMap::new(),
        });
        let source = "[project]\nname = \"test\"\n\n[compose]\nfile = \"\"\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::EmptyComposeFile { .. }
        ));
    }

    #[test]
    fn infra_named_ports_conflict_detected() {
        let mut config = make_config(vec![("api", "cargo run", Some(Port::Fixed(8025)), vec![])]);
        let mut mailpit = make_infra("axllent/mailpit:latest", None, vec![]);
        mailpit.ports.insert("smtp".to_string(), Port::Fixed(1025));
        mailpit.ports.insert("ui".to_string(), Port::Fixed(8025));
        config.infra.insert("mailpit".to_string(), mailpit);
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 8025\n\n[infra.mailpit]\nimage = \"axllent/mailpit:latest\"\n[infra.mailpit.ports]\nsmtp = 1025\nui = 8025\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigDiagnostic::DuplicatePort { port: 8025, services, .. } if services.len() == 2
        ));
    }

    #[test]
    fn infra_cycle_detected() {
        let mut config = make_config(vec![]);
        config
            .infra
            .insert("a".to_string(), make_infra("img-a", None, vec!["b"]));
        config
            .infra
            .insert("b".to_string(), make_infra("img-b", None, vec!["a"]));
        let source = "[project]\nname = \"test\"\n\n[infra.a]\nimage = \"img-a\"\ndepends_on = [\"b\"]\n\n[infra.b]\nimage = \"img-b\"\ndepends_on = [\"a\"]\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        let has_cycle = errs
            .iter()
            .any(|e| matches!(e, ConfigDiagnostic::DependencyCycle { .. }));
        assert!(
            has_cycle,
            "expected a DependencyCycle error for infra a->b->a"
        );
    }

    #[test]
    fn service_depends_on_compose_service_is_valid() {
        let mut config = make_config(vec![(
            "api",
            "cargo run",
            Some(Port::Fixed(3000)),
            vec!["redis"],
        )]);
        config.compose = Some(ComposeConfig {
            file: "docker-compose.yml".to_string(),
            services: vec!["redis".to_string(), "postgres".to_string()],
            env_file: None,
            ready_checks: BTreeMap::new(),
        });
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\nport = 3000\ndepends_on = [\"redis\"]\n\n[compose]\nfile = \"docker-compose.yml\"\nservices = [\"redis\", \"postgres\"]\n";
        assert!(validate(&config, source, TEST_FILENAME).is_ok());
    }

    // --- v0.3 cluster validation tests ---

    fn make_deploy(context: &str, manifests: &str, deps: Vec<&str>) -> ClusterDeployConfig {
        ClusterDeployConfig {
            context: context.to_string(),
            dockerfile: "Dockerfile".to_string(),
            manifests: manifests.to_string(),
            watch: false,
            depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn service_depends_on_cluster_deploy_name_is_valid() {
        let mut config = make_config(vec![(
            "web",
            "npm run dev",
            Some(Port::Fixed(3000)),
            vec!["api"],
        )]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: true,
            deploy: BTreeMap::from([("api".to_string(), make_deploy("./api", "./k8s", vec![]))]),
        });
        let source = "[project]\nname = \"test\"\n\n[services.web]\ncommand = \"npm run dev\"\nport = 3000\ndepends_on = [\"api\"]\n\n[cluster]\nregistry = true\n\n[cluster.deploy.api]\ncontext = \"./api\"\nmanifests = \"./k8s\"\n";
        assert!(validate(&config, source, TEST_FILENAME).is_ok());
    }

    #[test]
    fn cluster_deploy_depends_on_infra_is_valid() {
        let mut config = make_config(vec![]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: true,
            deploy: BTreeMap::from([(
                "api".to_string(),
                make_deploy("./api", "./k8s", vec!["postgres"]),
            )]),
        });
        let source = "[project]\nname = \"test\"\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n\n[cluster]\nregistry = true\n\n[cluster.deploy.api]\ncontext = \"./api\"\nmanifests = \"./k8s\"\ndepends_on = [\"postgres\"]\n";
        assert!(validate(&config, source, TEST_FILENAME).is_ok());
    }

    #[test]
    fn cluster_deploy_with_empty_context_errors() {
        let mut config = make_config(vec![]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([("api".to_string(), make_deploy("", "./k8s", vec![]))]),
        });
        let source = "[project]\nname = \"test\"\n\n[cluster.deploy.api]\ncontext = \"\"\nmanifests = \"./k8s\"\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ConfigDiagnostic::EmptyDeployContext { deploy, .. } if deploy == "api"
        )));
    }

    #[test]
    fn cluster_deploy_with_empty_manifests_errors() {
        let mut config = make_config(vec![]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([("api".to_string(), make_deploy("./api", "", vec![]))]),
        });
        let source = "[project]\nname = \"test\"\n\n[cluster.deploy.api]\ncontext = \"./api\"\nmanifests = \"\"\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ConfigDiagnostic::EmptyDeployManifests { deploy, .. } if deploy == "api"
        )));
    }

    #[test]
    fn cluster_deploy_name_conflicts_with_infra_name_errors() {
        let mut config = make_config(vec![]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("postgres:16-alpine", Some(Port::Fixed(5432)), vec![]),
        );
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([(
                "postgres".to_string(),
                make_deploy("./pg", "./k8s", vec![]),
            )]),
        });
        let source = "[project]\nname = \"test\"\n\n[infra.postgres]\nimage = \"postgres:16-alpine\"\nport = 5432\n\n[cluster.deploy.postgres]\ncontext = \"./pg\"\nmanifests = \"./k8s\"\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ConfigDiagnostic::DuplicateResourceName { name, .. } if name == "postgres"
        )));
    }

    #[test]
    fn cluster_deploy_depends_on_unknown_name_errors() {
        let mut config = make_config(vec![]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([(
                "api".to_string(),
                make_deploy("./api", "./k8s", vec!["nonexistent"]),
            )]),
        });
        let source = "[project]\nname = \"test\"\n\n[cluster.deploy.api]\ncontext = \"./api\"\nmanifests = \"./k8s\"\ndepends_on = [\"nonexistent\"]\n";
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ConfigDiagnostic::MissingDependency {
                service,
                dependency,
                ..
            } if service == "api" && dependency == "nonexistent"
        )));
    }

    // --- v0.4 validation diagnostic tests ---

    #[test]
    fn diagnostics_implement_miette_diagnostic() {
        let source = "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\ndepends_on = [\"db\"]\n";
        let config: DevrigConfig = toml::from_str(source).unwrap();
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        // Each error should render correctly via Display
        for err in &errs {
            let msg = format!("{}", err);
            assert!(!msg.is_empty());
            // Verify it has a diagnostic code
            let diag: &dyn miette::Diagnostic = err;
            assert!(diag.code().is_some());
        }
    }

    #[test]
    fn invalid_restart_policy_detected() {
        let source = r#"
[project]
name = "test"

[services.api]
command = "cargo run"

[services.api.restart]
policy = "invalid"
"#;
        let config: DevrigConfig = toml::from_str(source).unwrap();
        let errs = validate(&config, source, TEST_FILENAME).unwrap_err();
        assert!(errs.iter().any(|e| matches!(
            e,
            ConfigDiagnostic::InvalidRestartPolicy { service, value, .. }
                if service == "api" && value == "invalid"
        )));
    }

    #[test]
    fn valid_restart_policies_accepted() {
        for policy in &["always", "on-failure", "never"] {
            let source = format!(
                "[project]\nname = \"test\"\n\n[services.api]\ncommand = \"cargo run\"\n\n[services.api.restart]\npolicy = \"{}\"\n",
                policy
            );
            let config: DevrigConfig = toml::from_str(&source).unwrap();
            assert!(
                validate(&config, &source, TEST_FILENAME).is_ok(),
                "policy '{}' should be valid",
                policy
            );
        }
    }
}
