use devrig::config::resolve::find_config;
use tempfile::TempDir;

#[test]
fn config_found_in_parent_directory() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("devrig.toml");
    std::fs::write(
        &config_path,
        r#"
[project]
name = "test-discovery"
"#,
    )
    .unwrap();

    // Create a subdirectory
    let sub = dir.path().join("subdir");
    std::fs::create_dir(&sub).unwrap();

    // find_config from subdir should find the parent's config
    let found = find_config(&sub, "devrig.toml");
    assert!(found.is_some(), "Should have found config in parent dir");
    assert_eq!(found.unwrap(), config_path);
}

#[test]
fn config_found_in_grandparent_directory() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("devrig.toml");
    std::fs::write(
        &config_path,
        r#"
[project]
name = "test-discovery-gp"
"#,
    )
    .unwrap();

    let sub = dir.path().join("a").join("b");
    std::fs::create_dir_all(&sub).unwrap();

    let found = find_config(&sub, "devrig.toml");
    assert!(found.is_some());
    assert_eq!(found.unwrap(), config_path);
}
