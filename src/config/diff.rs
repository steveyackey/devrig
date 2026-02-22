use std::collections::BTreeMap;

use crate::config::model::DevrigConfig;

/// Describes what changed between two config versions.
#[derive(Debug, Default)]
pub struct ConfigDiff {
    pub services_added: Vec<String>,
    pub services_removed: Vec<String>,
    pub services_changed: Vec<String>,
    pub infra_added: Vec<String>,
    pub infra_removed: Vec<String>,
    pub infra_changed: Vec<String>,
}

impl ConfigDiff {
    pub fn is_empty(&self) -> bool {
        self.services_added.is_empty()
            && self.services_removed.is_empty()
            && self.services_changed.is_empty()
            && self.infra_added.is_empty()
            && self.infra_removed.is_empty()
            && self.infra_changed.is_empty()
    }

    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.services_added.is_empty() {
            parts.push(format!("+{} services", self.services_added.len()));
        }
        if !self.services_removed.is_empty() {
            parts.push(format!("-{} services", self.services_removed.len()));
        }
        if !self.services_changed.is_empty() {
            parts.push(format!("~{} services", self.services_changed.len()));
        }
        if !self.infra_added.is_empty() {
            parts.push(format!("+{} infra", self.infra_added.len()));
        }
        if !self.infra_removed.is_empty() {
            parts.push(format!("-{} infra", self.infra_removed.len()));
        }
        if !self.infra_changed.is_empty() {
            parts.push(format!("~{} infra", self.infra_changed.len()));
        }
        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

fn diff_map<V: PartialEq>(
    old: &BTreeMap<String, V>,
    new: &BTreeMap<String, V>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for key in new.keys() {
        match old.get(key) {
            None => added.push(key.clone()),
            Some(old_val) => {
                if old_val != &new[key] {
                    changed.push(key.clone());
                }
            }
        }
    }
    for key in old.keys() {
        if !new.contains_key(key) {
            removed.push(key.clone());
        }
    }

    (added, removed, changed)
}

/// Compare two configs and produce a diff.
pub fn diff_configs(old: &DevrigConfig, new: &DevrigConfig) -> ConfigDiff {
    let (sa, sr, sc) = diff_map(&old.services, &new.services);
    let (ia, ir, ic) = diff_map(&old.infra, &new.infra);

    ConfigDiff {
        services_added: sa,
        services_removed: sr,
        services_changed: sc,
        infra_added: ia,
        infra_removed: ir,
        infra_changed: ic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{DevrigConfig, Port, ProjectConfig, ServiceConfig};

    fn minimal_config() -> DevrigConfig {
        DevrigConfig {
            project: ProjectConfig {
                name: "test".to_string(),
            },
            services: BTreeMap::new(),
            infra: BTreeMap::new(),
            compose: None,
            cluster: None,
            env: BTreeMap::new(),
            network: None,
        }
    }

    fn make_service(command: &str, port: Option<u16>) -> ServiceConfig {
        ServiceConfig {
            path: None,
            command: command.to_string(),
            port: port.map(Port::Fixed),
            env: BTreeMap::new(),
            depends_on: vec![],
            restart: None,
        }
    }

    #[test]
    fn no_changes() {
        let a = minimal_config();
        let b = minimal_config();
        let diff = diff_configs(&a, &b);
        assert!(diff.is_empty());
        assert_eq!(diff.summary(), "no changes");
    }

    #[test]
    fn service_added() {
        let a = minimal_config();
        let mut b = minimal_config();
        b.services
            .insert("api".to_string(), make_service("cargo run", Some(3000)));
        let diff = diff_configs(&a, &b);
        assert_eq!(diff.services_added, vec!["api"]);
        assert!(diff.services_removed.is_empty());
        assert!(diff.services_changed.is_empty());
    }

    #[test]
    fn service_removed() {
        let mut a = minimal_config();
        a.services
            .insert("api".to_string(), make_service("cargo run", Some(3000)));
        let b = minimal_config();
        let diff = diff_configs(&a, &b);
        assert!(diff.services_added.is_empty());
        assert_eq!(diff.services_removed, vec!["api"]);
    }

    #[test]
    fn service_changed() {
        let mut a = minimal_config();
        a.services
            .insert("api".to_string(), make_service("cargo run", Some(3000)));
        let mut b = minimal_config();
        b.services
            .insert("api".to_string(), make_service("cargo run", Some(4000)));
        let diff = diff_configs(&a, &b);
        assert!(diff.services_added.is_empty());
        assert!(diff.services_removed.is_empty());
        assert_eq!(diff.services_changed, vec!["api"]);
    }

    #[test]
    fn summary_format() {
        let mut diff = ConfigDiff::default();
        diff.services_added = vec!["web".to_string()];
        diff.services_changed = vec!["api".to_string()];
        diff.infra_removed = vec!["redis".to_string()];
        let s = diff.summary();
        assert!(s.contains("+1 services"));
        assert!(s.contains("~1 services"));
        assert!(s.contains("-1 infra"));
    }

    #[test]
    fn multiple_changes() {
        let mut a = minimal_config();
        a.services
            .insert("api".to_string(), make_service("echo old", Some(3000)));
        a.services
            .insert("worker".to_string(), make_service("echo work", None));

        let mut b = minimal_config();
        b.services
            .insert("api".to_string(), make_service("echo new", Some(3000)));
        b.services
            .insert("web".to_string(), make_service("npm dev", Some(8080)));

        let diff = diff_configs(&a, &b);
        assert_eq!(diff.services_added, vec!["web"]);
        assert_eq!(diff.services_removed, vec!["worker"]);
        assert_eq!(diff.services_changed, vec!["api"]);
    }
}
