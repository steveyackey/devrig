use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::BTreeMap;

use crate::config::model::DevrigConfig;

/// Resolves service startup order from the dependency graph declared in a DevrigConfig.
///
/// Edges point from dependency to dependent (i.e. if service B depends on service A,
/// the edge is A -> B). A topological sort then yields the correct startup order:
/// dependencies before the services that need them.
#[derive(Debug)]
pub struct DependencyResolver {
    graph: DiGraph<String, ()>,
    #[allow(dead_code)]
    node_map: BTreeMap<String, NodeIndex>,
}

impl DependencyResolver {
    /// Build a dependency graph from a DevrigConfig.
    ///
    /// Returns an error if any service lists a dependency that is not itself
    /// defined as a service in the config.
    pub fn from_config(config: &DevrigConfig) -> Result<Self, String> {
        let mut graph = DiGraph::new();
        let mut node_map = BTreeMap::new();

        // First pass: add every service as a node.
        for name in config.services.keys() {
            let idx = graph.add_node(name.clone());
            node_map.insert(name.clone(), idx);
        }

        // Second pass: add edges from dependency -> dependent.
        for (name, svc) in &config.services {
            let dependent_idx = node_map[name];
            for dep in &svc.depends_on {
                let dep_idx = node_map.get(dep).ok_or_else(|| {
                    format!(
                        "service '{}' depends on '{}', which is not defined",
                        name, dep
                    )
                })?;
                graph.add_edge(*dep_idx, dependent_idx, ());
            }
        }

        Ok(Self { graph, node_map })
    }

    /// Return a valid startup order (dependencies first).
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn start_order(&self) -> Result<Vec<String>, String> {
        match toposort(&self.graph, None) {
            Ok(indices) => Ok(indices
                .into_iter()
                .map(|idx| self.graph[idx].clone())
                .collect()),
            Err(cycle) => {
                let offending = &self.graph[cycle.node_id()];
                Err(format!(
                    "dependency cycle detected involving service '{}'",
                    offending
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{DevrigConfig, ProjectConfig, ServiceConfig};

    /// Build a DevrigConfig from a list of (service_name, dependencies) pairs.
    fn make_config(services: Vec<(&str, Vec<&str>)>) -> DevrigConfig {
        let mut svc_map = BTreeMap::new();
        for (name, deps) in services {
            svc_map.insert(
                name.to_string(),
                ServiceConfig {
                    path: None,
                    command: "echo test".to_string(),
                    port: None,
                    env: BTreeMap::new(),
                    depends_on: deps.into_iter().map(|d| d.to_string()).collect(),
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

    /// Assert that service `a` appears before service `b` in the given order.
    fn assert_before(order: &[String], a: &str, b: &str) {
        let pos_a = order
            .iter()
            .position(|s| s == a)
            .unwrap_or_else(|| panic!("'{}' not found in order {:?}", a, order));
        let pos_b = order
            .iter()
            .position(|s| s == b)
            .unwrap_or_else(|| panic!("'{}' not found in order {:?}", b, order));
        assert!(
            pos_a < pos_b,
            "expected '{}' (pos {}) before '{}' (pos {}) in {:?}",
            a,
            pos_a,
            b,
            pos_b,
            order
        );
    }

    #[test]
    fn linear_chain() {
        // a depends on b, b depends on c  =>  start order: c, b, a
        let config = make_config(vec![("a", vec!["b"]), ("b", vec!["c"]), ("c", vec![])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_eq!(order, vec!["c", "b", "a"]);
    }

    #[test]
    fn diamond_dependency() {
        // d depends on b and c; b and c both depend on a
        let config = make_config(vec![
            ("a", vec![]),
            ("b", vec!["a"]),
            ("c", vec!["a"]),
            ("d", vec!["b", "c"]),
        ]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "a", "b");
        assert_before(&order, "a", "c");
        assert_before(&order, "b", "d");
        assert_before(&order, "c", "d");
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn cycle_detected() {
        // a -> b -> c -> a  (cycle)
        let config = make_config(vec![("a", vec!["c"]), ("b", vec!["a"]), ("c", vec!["b"])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(
            err.contains("dependency cycle detected"),
            "unexpected error message: {}",
            err
        );
    }

    #[test]
    fn self_loop_detected() {
        let config = make_config(vec![("a", vec!["a"])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(
            err.contains("dependency cycle detected"),
            "unexpected error message: {}",
            err
        );
        assert!(
            err.contains("'a'"),
            "error should name the offending service: {}",
            err
        );
    }

    #[test]
    fn no_dependencies() {
        let config = make_config(vec![("alpha", vec![]), ("beta", vec![]), ("gamma", vec![])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_eq!(order.len(), 3);
        // All three must be present (order is deterministic because BTreeMap is sorted).
        assert!(order.contains(&"alpha".to_string()));
        assert!(order.contains(&"beta".to_string()));
        assert!(order.contains(&"gamma".to_string()));
    }

    #[test]
    fn empty_config() {
        let config = make_config(vec![]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn single_service() {
        let config = make_config(vec![("only", vec![])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_eq!(order, vec!["only"]);
    }

    #[test]
    fn unknown_dependency_errors() {
        let config = make_config(vec![("web", vec!["db"])]);
        let err = DependencyResolver::from_config(&config).unwrap_err();
        assert!(
            err.contains("'web'") && err.contains("'db'") && err.contains("not defined"),
            "unexpected error message: {}",
            err
        );
    }
}
