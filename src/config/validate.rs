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
}

pub fn validate(config: &DevrigConfig) -> Result<(), Vec<ConfigError>> {
    let mut errors = Vec::new();

    let available: Vec<String> = config.services.keys().cloned().collect();

    // Check all depends_on references exist
    for (name, svc) in &config.services {
        for dep in &svc.depends_on {
            if !config.services.contains_key(dep) {
                errors.push(ConfigError::MissingDependency {
                    service: name.clone(),
                    dependency: dep.clone(),
                    available: available.clone(),
                });
            }
        }
    }

    // Check no two services share the same fixed port
    let mut port_map: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for (name, svc) in &config.services {
        if let Some(Port::Fixed(p)) = &svc.port {
            port_map.entry(*p).or_default().push(name.clone());
        }
    }
    for (port, services) in port_map {
        if services.len() > 1 {
            errors.push(ConfigError::DuplicatePort { port, services });
        }
    }

    // Check for dependency cycles using iterative DFS with visited/in_stack
    {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for start in config.services.keys() {
            if visited.contains(start.as_str()) {
                continue;
            }

            // Stack entries: (node, index into depends_on)
            let mut stack: Vec<(&str, usize)> = vec![(start.as_str(), 0)];
            in_stack.insert(start.as_str());

            while let Some((node, idx)) = stack.last_mut() {
                let deps = &config.services[*node].depends_on;
                if *idx < deps.len() {
                    let dep = deps[*idx].as_str();
                    *idx += 1;

                    // Only follow edges to services that actually exist
                    if !config.services.contains_key(dep) {
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

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ProjectConfig, ServiceConfig};

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
            env: BTreeMap::new(),
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
}
