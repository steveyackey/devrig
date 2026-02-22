use std::collections::{BTreeMap, HashSet};

use crate::config::model::{DevrigConfig, Port};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("service '{service}' depends on '{dependency}' which does not exist (available: {available:?})")]
    MissingDependency {
        service: String,
        dependency: String,
        available: Vec<String>,
    },

    #[error("port {port} is used by multiple services: {services:?}")]
    DuplicatePort { port: u16, services: Vec<String> },

    #[error("dependency cycle detected involving '{node}'")]
    DependencyCycle { node: String },

    #[error("service '{service}' has an empty command")]
    EmptyCommand { service: String },

    #[error("infra '{service}' has an empty image")]
    EmptyImage { service: String },

    #[error("compose.file is empty")]
    EmptyComposeFile,
}

pub fn validate(config: &DevrigConfig) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    // Build the list of all available names: services + infra + compose.services
    let mut available: Vec<String> = config.services.keys().cloned().collect();
    for name in config.infra.keys() {
        available.push(name.clone());
    }
    if let Some(compose) = &config.compose {
        for svc in &compose.services {
            available.push(svc.clone());
        }
    }

    // Check all depends_on references exist (services)
    for (name, svc) in &config.services {
        for dep in &svc.depends_on {
            if !available.contains(dep) {
                errors.push(ConfigError::MissingDependency {
                    service: name.clone(),
                    dependency: dep.clone(),
                    available: available.clone(),
                });
            }
        }
    }

    // Check all depends_on references exist (infra)
    for (name, infra) in &config.infra {
        for dep in &infra.depends_on {
            if !available.contains(dep) {
                errors.push(ConfigError::MissingDependency {
                    service: name.clone(),
                    dependency: dep.clone(),
                    available: available.clone(),
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
            errors.push(ConfigError::DuplicatePort { port, services });
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

    // Check for dependency cycles using iterative DFS with visited/in_stack
    {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for start in deps_map.keys() {
            if visited.contains(start) {
                continue;
            }

            // Stack entries: (node, index into depends_on)
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            in_stack.insert(start);

            while let Some((node, idx)) = stack.last_mut() {
                let deps = deps_map[*node];
                if *idx < deps.len() {
                    let dep = deps[*idx].as_str();
                    *idx += 1;

                    // Only follow edges to nodes that actually exist in the deps map
                    if !deps_map.contains_key(dep) {
                        continue;
                    }

                    if in_stack.contains(dep) {
                        errors.push(ConfigError::DependencyCycle {
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
            errors.push(ConfigError::EmptyCommand {
                service: name.clone(),
            });
        }
    }

    // Check no infra entry has an empty image string
    for (name, infra) in &config.infra {
        if infra.image.trim().is_empty() {
            errors.push(ConfigError::EmptyImage {
                service: name.clone(),
            });
        }
    }

    // Check compose.file is non-empty if compose is present
    if let Some(compose) = &config.compose {
        if compose.file.trim().is_empty() {
            errors.push(ConfigError::EmptyComposeFile);
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
    use crate::config::model::{ComposeConfig, InfraConfig, ProjectConfig, ServiceConfig};

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
            env: BTreeMap::new(),
            network: None,
        }
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
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::MissingDependency {
                service,
                dependency,
                ..
            } if service == "api" && dependency == "db"
        ));
    }

    #[test]
    fn duplicate_ports_detected() {
        let config = make_config(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec![]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::DuplicatePort { port: 3000, services } if services.len() == 2
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
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn multiple_errors_collected() {
        let config = make_config(vec![
            ("api", "cargo run", Some(Port::Fixed(3000)), vec!["redis"]),
            ("web", "npm start", Some(Port::Fixed(3000)), vec![]),
        ]);
        let errs = validate(&config).unwrap_err();
        assert!(errs.len() >= 2);

        let has_missing_dep = errs
            .iter()
            .any(|e| matches!(e, ConfigError::MissingDependency { .. }));
        let has_dup_port = errs
            .iter()
            .any(|e| matches!(e, ConfigError::DuplicatePort { .. }));
        assert!(has_missing_dep, "expected a MissingDependency error");
        assert!(has_dup_port, "expected a DuplicatePort error");
    }

    #[test]
    fn empty_command_detected() {
        let config = make_config(vec![("api", "  ", Some(Port::Fixed(3000)), vec![])]);
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::EmptyCommand { service } if service == "api"
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
        let errs = validate(&config).unwrap_err();
        let has_cycle = errs.iter().any(|e| {
            matches!(
                e,
                ConfigError::DependencyCycle { node } if node == "api"
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
        let errs = validate(&config).unwrap_err();
        let has_cycle = errs
            .iter()
            .any(|e| matches!(e, ConfigError::DependencyCycle { .. }));
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
        assert!(validate(&config).is_ok());
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
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::MissingDependency {
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
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::DuplicatePort { port: 5432, services } if services.len() == 2
        ));
    }

    #[test]
    fn infra_with_empty_image_errors() {
        let mut config = make_config(vec![]);
        config.infra.insert(
            "postgres".to_string(),
            make_infra("", Some(Port::Fixed(5432)), vec![]),
        );
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::EmptyImage { service } if service == "postgres"
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
        assert!(validate(&config).is_ok());
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
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], ConfigError::EmptyComposeFile));
    }

    #[test]
    fn infra_named_ports_conflict_detected() {
        let mut config = make_config(vec![("api", "cargo run", Some(Port::Fixed(8025)), vec![])]);
        let mut mailpit = make_infra("axllent/mailpit:latest", None, vec![]);
        mailpit.ports.insert("smtp".to_string(), Port::Fixed(1025));
        mailpit.ports.insert("ui".to_string(), Port::Fixed(8025));
        config.infra.insert("mailpit".to_string(), mailpit);
        let errs = validate(&config).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ConfigError::DuplicatePort { port: 8025, services } if services.len() == 2
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
        let errs = validate(&config).unwrap_err();
        let has_cycle = errs
            .iter()
            .any(|e| matches!(e, ConfigError::DependencyCycle { .. }));
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
        assert!(validate(&config).is_ok());
    }
}
