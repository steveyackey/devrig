use crate::common::{docker_cleanup, k3d_available, k3d_cleanup_sync, wait_for_job_complete};
use std::time::Duration;

#[tokio::test]
async fn cluster_network_bridge() {
    if !k3d_available() {
        eprintln!("Skipping: k3d not found");
        return;
    }

    let slug = "nettest";
    let cluster_name = format!("devrig-{}", slug);
    let network_name = format!("devrig-{}-net", slug);
    let redis_name = format!("devrig-{}-redis", slug);

    // Pre-test cleanup: remove stale resources from previous runs
    k3d_cleanup_sync(&cluster_name);
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", &redis_name])
        .output();
    docker_cleanup(slug);
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", &network_name])
        .output();

    // Use sync cleanup in scopeguard to avoid "Cannot start a runtime from
    // within a runtime" panic if the guard fires inside the tokio test runtime.
    let guard_cluster = cluster_name.clone();
    let guard_redis = redis_name.clone();
    let guard_network = network_name.clone();
    let _guard = scopeguard::guard((), move |_| {
        k3d_cleanup_sync(&guard_cluster);
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &guard_redis])
            .output();
        docker_cleanup(slug);
        let _ = std::process::Command::new("docker")
            .args(["network", "rm", &guard_network])
            .output();
    });

    // Create network
    let status = std::process::Command::new("docker")
        .args(["network", "create", &network_name])
        .status()
        .expect("docker network create");
    assert!(status.success(), "failed to create network");

    // Start a Redis container on the network
    let status = std::process::Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &redis_name,
            "--network",
            &network_name,
            "redis:7-alpine",
        ])
        .status()
        .expect("start redis");
    assert!(status.success(), "failed to start redis container");

    // Wait for redis to be ready
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Create k3d cluster on the same network
    let cluster_config = devrig::config::model::ClusterConfig {
        name: None,
        agents: 0,
        ports: vec![],
        registry: false,
        deploy: std::collections::BTreeMap::new(),
    };

    let state_dir = std::env::temp_dir().join(format!("devrig-nettest-{}", std::process::id()));
    std::fs::create_dir_all(&state_dir).unwrap();

    let k3d_mgr =
        devrig::cluster::K3dManager::new(slug, &cluster_config, &state_dir, &network_name);

    k3d_mgr.create_cluster().await.expect("create cluster");
    k3d_mgr.write_kubeconfig().await.expect("write kubeconfig");

    // Wait for cluster to be ready
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Create a Job that tries to connect to redis.
    // k3d injects Docker container names into CoreDNS NodeHosts, so pods can
    // resolve Docker containers on the shared network without hostNetwork.
    let job_yaml = format!(
        r#"
apiVersion: batch/v1
kind: Job
metadata:
  name: checker
spec:
  template:
    spec:
      containers:
      - name: checker
        image: alpine:3.19
        command: ["sh", "-c", "apk add --no-cache netcat-openbsd && nc -zv {} 6379"]
      restartPolicy: Never
  backoffLimit: 3
"#,
        redis_name
    );

    let manifest_path = state_dir.join("checker-job.yaml");
    std::fs::write(&manifest_path, &job_yaml).unwrap();

    k3d_mgr
        .kubectl(&["apply", "-f", &manifest_path.to_string_lossy()])
        .await
        .expect("apply checker job");

    // Wait for job to complete
    let completed = wait_for_job_complete(
        k3d_mgr.kubeconfig_path(),
        "checker",
        Duration::from_secs(90),
    )
    .await;

    assert!(
        completed,
        "checker job did not complete - network bridge may not work"
    );

    // Cleanup
    k3d_mgr.delete_cluster().await.expect("delete cluster");
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", &redis_name])
        .output();
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", &network_name])
        .output();
    let _ = std::fs::remove_dir_all(&state_dir);

    std::mem::forget(_guard);
}
