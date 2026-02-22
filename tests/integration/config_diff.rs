#[test]
fn diff_detects_service_addition() {
    use devrig::config::diff::diff_configs;

    let toml_a = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"
    "#;

    let toml_b = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"

        [services.web]
        command = "echo b"
    "#;

    let config_a: devrig::config::model::DevrigConfig = toml::from_str(toml_a).unwrap();
    let config_b: devrig::config::model::DevrigConfig = toml::from_str(toml_b).unwrap();

    let diff = diff_configs(&config_a, &config_b);
    assert_eq!(diff.services_added, vec!["web"]);
    assert!(diff.services_removed.is_empty());
    assert!(diff.services_changed.is_empty());
    assert!(!diff.is_empty());
}

#[test]
fn diff_detects_service_removal() {
    use devrig::config::diff::diff_configs;

    let toml_a = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"

        [services.web]
        command = "echo b"
    "#;

    let toml_b = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"
    "#;

    let config_a: devrig::config::model::DevrigConfig = toml::from_str(toml_a).unwrap();
    let config_b: devrig::config::model::DevrigConfig = toml::from_str(toml_b).unwrap();

    let diff = diff_configs(&config_a, &config_b);
    assert!(diff.services_added.is_empty());
    assert_eq!(diff.services_removed, vec!["web"]);
}

#[test]
fn diff_detects_service_change() {
    use devrig::config::diff::diff_configs;

    let toml_a = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo old"
        port = 3000
    "#;

    let toml_b = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo new"
        port = 4000
    "#;

    let config_a: devrig::config::model::DevrigConfig = toml::from_str(toml_a).unwrap();
    let config_b: devrig::config::model::DevrigConfig = toml::from_str(toml_b).unwrap();

    let diff = diff_configs(&config_a, &config_b);
    assert!(diff.services_added.is_empty());
    assert!(diff.services_removed.is_empty());
    assert_eq!(diff.services_changed, vec!["api"]);
}

#[test]
fn diff_no_changes() {
    use devrig::config::diff::diff_configs;

    let toml = r#"
        [project]
        name = "test"

        [services.api]
        command = "echo a"
        port = 3000
    "#;

    let config_a: devrig::config::model::DevrigConfig = toml::from_str(toml).unwrap();
    let config_b: devrig::config::model::DevrigConfig = toml::from_str(toml).unwrap();

    let diff = diff_configs(&config_a, &config_b);
    assert!(diff.is_empty());
    assert_eq!(diff.summary(), "no changes");
}
