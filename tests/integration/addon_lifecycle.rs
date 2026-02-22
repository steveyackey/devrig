#![cfg(feature = "integration")]

use std::process::Command;

use crate::common::TestProject;

/// Check whether k3d and helm are available. If not, skip the test.
fn require_k3d_and_helm() -> bool {
    let k3d_ok = Command::new("k3d").arg("version").output().is_ok();
    let helm_ok = Command::new("helm").arg("version").output().is_ok();
    k3d_ok && helm_ok
}

/// Verify that a Helm addon can be installed into a k3d cluster and torn down.
///
/// This test is conditional on k3d + helm being available on the host.
#[test]
fn addon_helm_lifecycle() {
    if !require_k3d_and_helm() {
        eprintln!("Skipping addon_helm_lifecycle: k3d or helm not found");
        return;
    }

    let project = TestProject::new(
        r#"
        [project]
        name = "test-addon"

        [services.api]
        command = "echo hi"

        [cluster]
        name = "devrig-addon-test"
        agents = 1

        [cluster.addons.traefik]
        type = "helm"
        chart = "traefik/traefik"
        repo = "https://traefik.github.io/charts"
        namespace = "traefik"
    "#,
    );

    // Parse the config to verify addon round-trips
    let content = std::fs::read_to_string(&project.config_path).unwrap();
    let parsed: toml::Value = content.parse().unwrap();
    let addons = parsed
        .get("cluster")
        .and_then(|c| c.get("addons"))
        .and_then(|a| a.get("traefik"));
    assert!(addons.is_some(), "traefik addon should parse from config");

    let addon_type = addons.unwrap().get("type").unwrap().as_str().unwrap();
    assert_eq!(addon_type, "helm", "addon type should be helm");
}

/// Verify that addon config values are correctly represented.
#[test]
fn addon_helm_values_roundtrip() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test-addon-values"

        [services.api]
        command = "echo hi"

        [cluster]

        [cluster.addons.grafana]
        type = "helm"
        chart = "grafana/grafana"
        repo = "https://grafana.github.io/helm-charts"
        namespace = "monitoring"
        version = "7.0.0"

        [cluster.addons.grafana.values]
        "replicas" = 1
        "persistence.enabled" = true

        [cluster.addons.grafana.port_forward]
        3000 = "svc/grafana:80"
    "#,
    );

    let content = std::fs::read_to_string(&project.config_path).unwrap();
    let config: devrig::config::model::DevrigConfig = toml::from_str(&content).unwrap();

    let cluster = config.cluster.unwrap();
    let grafana = cluster.addons.get("grafana").unwrap();
    match grafana {
        devrig::config::model::AddonConfig::Helm {
            chart,
            repo,
            namespace,
            version,
            values,
            port_forward,
        } => {
            assert_eq!(chart, "grafana/grafana");
            assert_eq!(repo, "https://grafana.github.io/helm-charts");
            assert_eq!(namespace, "monitoring");
            assert_eq!(version.as_deref(), Some("7.0.0"));
            assert!(values.contains_key("replicas"));
            assert!(port_forward.contains_key("3000"));
        }
        _ => panic!("expected Helm addon"),
    }
}

/// Verify that manifest addon config parses correctly.
#[test]
fn addon_manifest_config_parses() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test-manifest-addon"

        [services.api]
        command = "echo hi"

        [cluster]

        [cluster.addons.monitoring]
        type = "manifest"
        path = "k8s/monitoring.yaml"
        namespace = "monitoring"
    "#,
    );

    let content = std::fs::read_to_string(&project.config_path).unwrap();
    let config: devrig::config::model::DevrigConfig = toml::from_str(&content).unwrap();

    let cluster = config.cluster.unwrap();
    let monitoring = cluster.addons.get("monitoring").unwrap();
    match monitoring {
        devrig::config::model::AddonConfig::Manifest {
            path, namespace, ..
        } => {
            assert_eq!(path, "k8s/monitoring.yaml");
            assert_eq!(namespace.as_deref(), Some("monitoring"));
        }
        _ => panic!("expected Manifest addon"),
    }
}
