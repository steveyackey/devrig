use crate::common::TestProject;

#[test]
fn validate_valid_config() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test"

        [services.api]
        command = "echo hi"
        port = 3000
    "#,
    );

    let (config, source) = devrig::config::load_config(&project.config_path).unwrap();
    let filename = project
        .config_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(devrig::config::validate::validate(&config, &source, &filename).is_ok());
}

#[test]
fn validate_catches_duplicate_ports() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"
        port = 3000

        [services.web]
        command = "echo b"
        port = 3000
    "#,
    );

    let (config, source) = devrig::config::load_config(&project.config_path).unwrap();
    let filename = project
        .config_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(devrig::config::validate::validate(&config, &source, &filename).is_err());
}

#[test]
fn validate_catches_missing_dependency() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"
        depends_on = ["nonexistent"]
    "#,
    );

    let (config, source) = devrig::config::load_config(&project.config_path).unwrap();
    let filename = project
        .config_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(devrig::config::validate::validate(&config, &source, &filename).is_err());
}

#[test]
fn validate_catches_invalid_restart_policy() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test"

        [services.api]
        command = "echo hi"

        [services.api.restart]
        policy = "banana"
    "#,
    );

    let (config, source) = devrig::config::load_config(&project.config_path).unwrap();
    let filename = project
        .config_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(devrig::config::validate::validate(&config, &source, &filename).is_err());
}
