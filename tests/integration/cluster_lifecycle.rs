use crate::common::{docker_cleanup, file_checksum, k3d_available, k3d_cleanup_sync};

#[tokio::test]
async fn cluster_lifecycle() {
    if !k3d_available() {
        eprintln!("Skipping: k3d not found");
        return;
    }

    // Checksum ~/.kube/config before test
    let home = std::env::var("HOME").unwrap_or_default();
    let kube_config_path = std::path::PathBuf::from(&home).join(".kube/config");
    let kube_checksum_before = file_checksum(&kube_config_path);

    let project = crate::common::TestProject::new(
        r#"
        [project]
        name = "cltest"

        [cluster]
        registry = true

        [cluster.deploy.echo]
        context = "./echo"
        manifests = "./k8s/echo"
    "#,
    );

    // Create Docker context for a simple echo service
    let echo_dir = project.dir.path().join("echo");
    std::fs::create_dir_all(&echo_dir).unwrap();
    std::fs::write(
        echo_dir.join("Dockerfile"),
        "FROM alpine:3.19\nCMD [\"sleep\", \"3600\"]\n",
    )
    .unwrap();

    // Create Kubernetes manifests
    let k8s_dir = project.dir.path().join("k8s/echo");
    std::fs::create_dir_all(&k8s_dir).unwrap();
    std::fs::write(
        k8s_dir.join("deployment.yaml"),
        r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: echo
  labels:
    app: echo
spec:
  replicas: 1
  selector:
    matchLabels:
      app: echo
  template:
    metadata:
      labels:
        app: echo
    spec:
      containers:
      - name: echo
        image: alpine:3.19
        command: ["sleep", "3600"]
"#,
    )
    .unwrap();

    let (config, _source) = devrig::config::load_config(&project.config_path).unwrap();
    let identity =
        devrig::identity::ProjectIdentity::from_config(&config, &project.config_path).unwrap();
    let slug = identity.slug.clone();

    let cluster_name = format!("devrig-{}", slug);
    let network_name = format!("devrig-{}-net", slug);

    // Use sync cleanup in scopeguard to avoid "Cannot start a runtime from
    // within a runtime" panic if the guard fires inside the tokio test runtime.
    let guard_cluster = cluster_name.clone();
    let guard_slug = slug.clone();
    let guard_network = network_name.clone();
    let _guard = scopeguard::guard((), move |_| {
        k3d_cleanup_sync(&guard_cluster);
        docker_cleanup(&guard_slug);
        let _ = std::process::Command::new("docker")
            .args(["network", "rm", &guard_network])
            .output();
    });

    // Create network first
    let _ = std::process::Command::new("docker")
        .args(["network", "create", &network_name])
        .output();

    let cluster_config = config.cluster.as_ref().unwrap();
    let state_dir = project.dir.path().join(".devrig");
    std::fs::create_dir_all(&state_dir).unwrap();

    let k3d_mgr =
        devrig::cluster::K3dManager::new(&slug, cluster_config, &state_dir, &network_name);

    // Create cluster
    k3d_mgr
        .create_cluster()
        .await
        .expect("cluster create failed");

    // Verify cluster exists
    assert!(
        k3d_mgr
            .cluster_exists()
            .await
            .expect("cluster_exists failed"),
        "cluster should exist after create"
    );

    // Write kubeconfig (with port fix)
    k3d_mgr
        .write_kubeconfig()
        .await
        .expect("write_kubeconfig failed");
    assert!(
        k3d_mgr.kubeconfig_path().exists(),
        "kubeconfig should exist"
    );

    // Verify kubectl works with the kubeconfig
    let nodes = k3d_mgr.kubectl(&["get", "nodes", "-o", "name"]).await;
    assert!(nodes.is_ok(), "kubectl get nodes failed: {:?}", nodes.err());

    // Verify ~/.kube/config is untouched
    let kube_checksum_after = file_checksum(&kube_config_path);
    assert_eq!(
        kube_checksum_before, kube_checksum_after,
        "~/.kube/config was modified!"
    );

    // Delete cluster
    k3d_mgr
        .delete_cluster()
        .await
        .expect("cluster delete failed");

    // Verify cluster is gone
    assert!(
        !k3d_mgr
            .cluster_exists()
            .await
            .expect("cluster_exists failed"),
        "cluster should not exist after delete"
    );

    // Verify kubeconfig removed
    assert!(
        !k3d_mgr.kubeconfig_path().exists(),
        "kubeconfig should be removed after delete"
    );

    // Cleanup network
    let _ = std::process::Command::new("docker")
        .args(["network", "rm", &network_name])
        .output();

    // Disarm the guard since we cleaned up successfully
    std::mem::forget(_guard);
}
