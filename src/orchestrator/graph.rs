use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::BTreeMap;

use crate::config::model::DevrigConfig;

/// The kind of resource represented by a node in the dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Service,
    Infra,
    Compose,
    ClusterDeploy,
}

/// A node in the unified dependency graph.
#[derive(Debug, Clone)]
pub struct ResourceNode {
    pub name: String,
    pub kind: ResourceKind,
}

/// Resolves startup order from the dependency graph declared in a DevrigConfig.
///
/// The graph is unified: it contains service nodes, infra nodes, and compose
/// service nodes. Edges point from dependency to dependent (i.e. if service B
/// depends on infra A, the edge is A -> B). A topological sort yields the
/// correct startup order: dependencies before their dependents.
#[derive(Debug)]
pub struct DependencyResolver {
    graph: DiGraph<ResourceNode, ()>,
    node_map: BTreeMap<String, NodeIndex>,
}

impl DependencyResolver {
    /// Build a unified dependency graph from a DevrigConfig.
    ///
    /// Includes services, infra, and compose service nodes. Returns an error
    /// if any node lists a dependency that is not defined anywhere.
    pub fn from_config(config: &DevrigConfig) -> Result<Self, String> {
        let mut graph = DiGraph::new();
        let mut node_map = BTreeMap::new();

        // Add infra nodes
        for name in config.infra.keys() {
            let idx = graph.add_node(ResourceNode {
                name: name.clone(),
                kind: ResourceKind::Infra,
            });
            node_map.insert(name.clone(), idx);
        }

        // Add compose service nodes
        if let Some(compose) = &config.compose {
            for svc_name in &compose.services {
                if !node_map.contains_key(svc_name) {
                    let idx = graph.add_node(ResourceNode {
                        name: svc_name.clone(),
                        kind: ResourceKind::Compose,
                    });
                    node_map.insert(svc_name.clone(), idx);
                }
            }
        }

        // Add cluster deploy nodes
        if let Some(cluster) = &config.cluster {
            for name in cluster.deploy.keys() {
                if !node_map.contains_key(name) {
                    let idx = graph.add_node(ResourceNode {
                        name: name.clone(),
                        kind: ResourceKind::ClusterDeploy,
                    });
                    node_map.insert(name.clone(), idx);
                }
            }
        }

        // Add service nodes
        for name in config.services.keys() {
            let idx = graph.add_node(ResourceNode {
                name: name.clone(),
                kind: ResourceKind::Service,
            });
            node_map.insert(name.clone(), idx);
        }

        // Add edges for infra depends_on
        for (name, infra) in &config.infra {
            let dependent_idx = node_map[name];
            for dep in &infra.depends_on {
                let dep_idx = node_map.get(dep).ok_or_else(|| {
                    format!(
                        "infra '{}' depends on '{}', which is not defined",
                        name, dep
                    )
                })?;
                graph.add_edge(*dep_idx, dependent_idx, ());
            }
        }

        // Add edges for cluster deploy depends_on
        if let Some(cluster) = &config.cluster {
            for (name, deploy) in &cluster.deploy {
                let dependent_idx = node_map[name];
                for dep in &deploy.depends_on {
                    let dep_idx = node_map.get(dep).ok_or_else(|| {
                        format!(
                            "cluster deploy '{}' depends on '{}', which is not defined",
                            name, dep
                        )
                    })?;
                    graph.add_edge(*dep_idx, dependent_idx, ());
                }
            }
        }

        // Add edges for service depends_on
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

    /// Return a valid startup order (dependencies first) with resource kind info.
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn start_order(&self) -> Result<Vec<(String, ResourceKind)>, String> {
        match toposort(&self.graph, None) {
            Ok(indices) => Ok(indices
                .into_iter()
                .map(|idx| {
                    let node = &self.graph[idx];
                    (node.name.clone(), node.kind)
                })
                .collect()),
            Err(cycle) => {
                let offending = &self.graph[cycle.node_id()];
                Err(format!(
                    "dependency cycle detected involving '{}'",
                    offending.name
                ))
            }
        }
    }

    /// Return just the names in startup order (for backward compatibility).
    pub fn start_order_names(&self) -> Result<Vec<String>, String> {
        self.start_order()
            .map(|order| order.into_iter().map(|(name, _)| name).collect())
    }

    /// Look up the resource kind for a given name.
    pub fn resource_kind(&self, name: &str) -> Option<ResourceKind> {
        self.node_map.get(name).map(|idx| self.graph[*idx].kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{
        ClusterConfig, ClusterDeployConfig, ComposeConfig, DevrigConfig, InfraConfig,
        ProjectConfig, ServiceConfig,
    };

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

    fn make_infra(image: &str, deps: Vec<&str>) -> InfraConfig {
        InfraConfig {
            image: image.to_string(),
            port: None,
            ports: BTreeMap::new(),
            env: BTreeMap::new(),
            volumes: Vec::new(),
            ready_check: None,
            init: Vec::new(),
            depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    fn assert_before(order: &[(String, ResourceKind)], a: &str, b: &str) {
        let pos_a = order
            .iter()
            .position(|(s, _)| s == a)
            .unwrap_or_else(|| panic!("'{}' not found in order {:?}", a, order));
        let pos_b = order
            .iter()
            .position(|(s, _)| s == b)
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

    fn names(order: &[(String, ResourceKind)]) -> Vec<String> {
        order.iter().map(|(n, _)| n.clone()).collect()
    }

    #[test]
    fn linear_chain() {
        let config = make_config(vec![("a", vec!["b"]), ("b", vec!["c"]), ("c", vec![])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_eq!(names(&order), vec!["c", "b", "a"]);
    }

    #[test]
    fn diamond_dependency() {
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
        let config = make_config(vec![("a", vec!["c"]), ("b", vec!["a"]), ("c", vec!["b"])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(err.contains("dependency cycle detected"));
    }

    #[test]
    fn self_loop_detected() {
        let config = make_config(vec![("a", vec!["a"])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(err.contains("dependency cycle detected"));
        assert!(err.contains("'a'"));
    }

    #[test]
    fn no_dependencies() {
        let config = make_config(vec![("alpha", vec![]), ("beta", vec![]), ("gamma", vec![])]);
        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order_names().unwrap();
        assert_eq!(order.len(), 3);
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
        let order = resolver.start_order_names().unwrap();
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

    #[test]
    fn mixed_graph_with_services_and_infra() {
        let mut config = make_config(vec![("api", vec!["postgres"]), ("worker", vec!["redis"])]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec![]));
        config
            .infra
            .insert("redis".into(), make_infra("redis:7", vec![]));

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "postgres", "api");
        assert_before(&order, "redis", "worker");
        assert_eq!(
            resolver.resource_kind("postgres"),
            Some(ResourceKind::Infra)
        );
        assert_eq!(resolver.resource_kind("redis"), Some(ResourceKind::Infra));
        assert_eq!(resolver.resource_kind("api"), Some(ResourceKind::Service));
        assert_eq!(
            resolver.resource_kind("worker"),
            Some(ResourceKind::Service)
        );
    }

    #[test]
    fn service_depends_on_infra() {
        let mut config = make_config(vec![("api", vec!["postgres", "redis"])]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec![]));
        config
            .infra
            .insert("redis".into(), make_infra("redis:7", vec![]));

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "postgres", "api");
        assert_before(&order, "redis", "api");
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn compose_nodes_before_dependent_services() {
        let mut config = make_config(vec![("api", vec!["redis"])]);
        config.compose = Some(ComposeConfig {
            file: "docker-compose.yml".to_string(),
            services: vec!["redis".to_string()],
            env_file: None,
            ready_checks: BTreeMap::new(),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "redis", "api");
        assert_eq!(resolver.resource_kind("redis"), Some(ResourceKind::Compose));
        assert_eq!(resolver.resource_kind("api"), Some(ResourceKind::Service));
    }

    #[test]
    fn infra_depends_on_another_infra() {
        let mut config = make_config(vec![("api", vec!["postgres"])]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec!["redis"]));
        config
            .infra
            .insert("redis".into(), make_infra("redis:7", vec![]));

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "redis", "postgres");
        assert_before(&order, "postgres", "api");
    }

    #[test]
    fn infra_cycle_detected() {
        let mut config = make_config(vec![]);
        config
            .infra
            .insert("a".into(), make_infra("img-a", vec!["b"]));
        config
            .infra
            .insert("b".into(), make_infra("img-b", vec!["a"]));

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(err.contains("dependency cycle detected"));
    }

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
    fn cluster_deploy_in_graph() {
        let mut config = make_config(vec![]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([("api".to_string(), make_deploy("./api", "./k8s", vec![]))]),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].0, "api");
        assert_eq!(order[0].1, ResourceKind::ClusterDeploy);
    }

    #[test]
    fn service_depends_on_cluster_deploy() {
        let mut config = make_config(vec![("web", vec!["api"])]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([("api".to_string(), make_deploy("./api", "./k8s", vec![]))]),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_before(&order, "api", "web");
        assert_eq!(
            resolver.resource_kind("api"),
            Some(ResourceKind::ClusterDeploy)
        );
        assert_eq!(resolver.resource_kind("web"), Some(ResourceKind::Service));
    }

    #[test]
    fn cluster_deploy_depends_on_infra() {
        let mut config = make_config(vec![]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec![]));
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([(
                "api".to_string(),
                make_deploy("./api", "./k8s", vec!["postgres"]),
            )]),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();
        assert_before(&order, "postgres", "api");
    }

    #[test]
    fn all_four_types_in_one_graph() {
        let mut config = make_config(vec![("web", vec!["api", "cache"])]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec![]));
        config.compose = Some(ComposeConfig {
            file: "docker-compose.yml".to_string(),
            services: vec!["cache".to_string()],
            env_file: None,
            ready_checks: BTreeMap::new(),
        });
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([(
                "api".to_string(),
                make_deploy("./api", "./k8s", vec!["postgres"]),
            )]),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_eq!(order.len(), 4);
        assert_before(&order, "postgres", "api");
        assert_before(&order, "api", "web");
        assert_before(&order, "cache", "web");
        assert_eq!(
            resolver.resource_kind("postgres"),
            Some(ResourceKind::Infra)
        );
        assert_eq!(
            resolver.resource_kind("api"),
            Some(ResourceKind::ClusterDeploy)
        );
        assert_eq!(resolver.resource_kind("cache"), Some(ResourceKind::Compose));
        assert_eq!(resolver.resource_kind("web"), Some(ResourceKind::Service));
    }

    #[test]
    fn cluster_deploy_cycle_detected() {
        let mut config = make_config(vec![]);
        config.cluster = Some(ClusterConfig {
            name: None,
            agents: 1,
            ports: vec![],
            registry: false,
            deploy: BTreeMap::from([
                ("a".to_string(), make_deploy("./a", "./k8s/a", vec!["b"])),
                ("b".to_string(), make_deploy("./b", "./k8s/b", vec!["a"])),
            ]),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let err = resolver.start_order().unwrap_err();
        assert!(err.contains("dependency cycle detected"));
    }

    #[test]
    fn cluster_deploy_unknown_dependency_errors() {
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

        let err = DependencyResolver::from_config(&config).unwrap_err();
        assert!(
            err.contains("'api'") && err.contains("'nonexistent'") && err.contains("not defined"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn all_three_types_in_one_graph() {
        let mut config = make_config(vec![("api", vec!["postgres", "cache"])]);
        config
            .infra
            .insert("postgres".into(), make_infra("postgres:16", vec![]));
        config.compose = Some(ComposeConfig {
            file: "docker-compose.yml".to_string(),
            services: vec!["cache".to_string()],
            env_file: None,
            ready_checks: BTreeMap::new(),
        });

        let resolver = DependencyResolver::from_config(&config).unwrap();
        let order = resolver.start_order().unwrap();

        assert_before(&order, "postgres", "api");
        assert_before(&order, "cache", "api");
        assert_eq!(
            resolver.resource_kind("postgres"),
            Some(ResourceKind::Infra)
        );
        assert_eq!(resolver.resource_kind("cache"), Some(ResourceKind::Compose));
        assert_eq!(resolver.resource_kind("api"), Some(ResourceKind::Service));
    }
}
