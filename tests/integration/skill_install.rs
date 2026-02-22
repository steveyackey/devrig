use crate::common::TestProject;

/// Verify that `skill install` writes the expected skill file to the project directory.
#[test]
fn skill_install_writes_to_project_dir() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test-skill"

        [services.api]
        command = "echo hi"
    "#,
    );

    // Run the skill install command using the library function
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        devrig::commands::skill::run_install(false, Some(project.config_path.as_path()))
            .await
            .unwrap();
    });

    // Check the skill file was written
    let skill_dir = project
        .dir
        .path()
        .join(".claude")
        .join("skills")
        .join("devrig");
    let skill_file = skill_dir.join("SKILL.md");
    assert!(skill_file.exists(), "SKILL.md should be created");

    let content = std::fs::read_to_string(&skill_file).unwrap();
    assert!(
        content.contains("name: devrig"),
        "should contain skill name"
    );
    assert!(
        content.contains("devrig query"),
        "should contain query commands"
    );
    assert!(
        content.contains("allowed-tools"),
        "should contain allowed-tools"
    );
}

/// Verify that `skill install --global` writes to ~/.claude/skills/devrig/.
#[test]
fn skill_install_global() {
    let fake_home = tempfile::TempDir::new().unwrap();

    // Override HOME so the global install targets our temp dir
    std::env::set_var("HOME", fake_home.path());

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        devrig::commands::skill::run_install(true, None)
            .await
            .unwrap();
    });

    let skill_file = fake_home
        .path()
        .join(".claude/skills/devrig/SKILL.md");
    assert!(skill_file.exists(), "SKILL.md should be created in global location");

    let content = std::fs::read_to_string(&skill_file).unwrap();
    assert!(
        content.contains("name: devrig"),
        "should contain skill name"
    );
}

/// Verify the skill install command is idempotent.
#[test]
fn skill_install_is_idempotent() {
    let project = TestProject::new(
        r#"
        [project]
        name = "test-idem"

        [services.api]
        command = "echo hi"
    "#,
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        devrig::commands::skill::run_install(false, Some(project.config_path.as_path()))
            .await
            .unwrap();
        devrig::commands::skill::run_install(false, Some(project.config_path.as_path()))
            .await
            .unwrap();
    });

    let skill_file = project.dir.path().join(".claude/skills/devrig/SKILL.md");
    assert!(
        skill_file.exists(),
        "SKILL.md should still exist after second install"
    );
}
