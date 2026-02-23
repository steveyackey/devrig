use crate::common::{docker_cleanup, k3d_available, k3d_cleanup_sync};

#[tokio::test]
async fn cluster_registry_push_pull() {
    if !k3d_available() {
        eprintln!("Skipping: k3d not found");
        return;
    }

    let slug = "regtest";
    let cluster_name = format!("devrig-{}", slug);
    let network_name = format!("devrig-{}-net", slug);

    // Pre-test cleanup: remove stale resources from previous runs
    k3d_cleanup_sync(&cluster_name);
    docker_cleanup(slug);
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", &network_name])
        .output();

    // Use sync cleanup in scopeguard to avoid runtime-in-runtime panics
    let guard_cluster = cluster_name.clone();
    let guard_network = network_name.clone();
    let _guard = scopeguard::guard((), move |_| {
        k3d_cleanup_sync(&guard_cluster);
        docker_cleanup(slug);
        let _ = std::process::Command::new("docker")
            .args(["network", "rm", &guard_network])
            .output();
    });

    // Create network
    let _ = std::process::Command::new("docker")
        .args(["network", "create", &network_name])
        .output();

    // Create cluster with registry using ClusterConfig directly
    let cluster_config = devrig::config::model::ClusterConfig {
        name: None,
        agents: 1,
        ports: vec![],
        registry: true,
        images: std::collections::BTreeMap::new(),
        deploy: std::collections::BTreeMap::new(),
        addons: std::collections::BTreeMap::new(),
        logs: None,
        registries: vec![],
    };

    let state_dir = std::env::temp_dir().join(format!("devrig-regtest-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).unwrap();

    let k3d_mgr =
        devrig::cluster::K3dManager::new(slug, &cluster_config, &state_dir, &network_name);

    k3d_mgr.create_cluster().await.expect("create cluster");
    k3d_mgr.write_kubeconfig().await.expect("write kubeconfig");

    // Discover registry port
    let port = devrig::cluster::registry::get_registry_port(slug)
        .await
        .expect("get registry port");
    assert!(port > 0, "registry port should be > 0");

    // Wait for registry
    devrig::cluster::registry::wait_for_registry(port)
        .await
        .expect("registry should become ready");

    // Build and push a test image
    let build_dir = state_dir.join("test-image");
    std::fs::create_dir_all(&build_dir).unwrap();
    std::fs::write(
        build_dir.join("Dockerfile"),
        "FROM alpine:3.19\nCMD [\"echo\", \"hello\"]\n",
    )
    .unwrap();

    let tag = format!("localhost:{}/hello:test", port);
    let status = tokio::process::Command::new("docker")
        .args(["build", "-t", &tag, "."])
        .current_dir(&build_dir)
        .status()
        .await
        .expect("docker build");
    assert!(status.success(), "docker build failed");

    let status = tokio::process::Command::new("docker")
        .args(["push", &tag])
        .status()
        .await
        .expect("docker push");
    assert!(status.success(), "docker push to local registry failed");

    // Cleanup
    k3d_mgr.delete_cluster().await.expect("delete cluster");
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", &network_name])
        .output();
    let _ = std::fs::remove_dir_all(&state_dir);

    std::mem::forget(_guard);
}
