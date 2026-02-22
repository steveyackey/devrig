use anyhow::{Context, Result};
use std::path::Path;

const SKILL_MD: &str = include_str!("../../skill/claude-code/SKILL.md");

pub async fn run_install(global: bool, config_file: Option<&Path>) -> Result<()> {
    let target = if global {
        let home = std::env::var("HOME").context("HOME environment variable not set")?;
        std::path::PathBuf::from(home).join(".claude/skills/devrig")
    } else {
        // Find the config directory: use config_file's parent, or walk up from CWD
        let config_dir = if let Some(cf) = config_file {
            cf.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
        } else {
            // Walk up from CWD to find devrig.toml, use its parent
            find_config_dir()?
        };
        config_dir.join(".claude/skills/devrig")
    };

    tokio::fs::create_dir_all(&target)
        .await
        .with_context(|| format!("creating directory {}", target.display()))?;

    tokio::fs::write(target.join("SKILL.md"), SKILL_MD)
        .await
        .with_context(|| format!("writing SKILL.md to {}", target.display()))?;

    println!("Installed devrig skill to {}", target.display());
    println!();
    println!("Try asking Claude: \"What services are running and are there any errors?\"");

    Ok(())
}

fn find_config_dir() -> Result<std::path::PathBuf> {
    let mut dir = std::env::current_dir().context("getting current directory")?;
    loop {
        if dir.join("devrig.toml").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            // Fallback to CWD if no devrig.toml found
            return std::env::current_dir().context("getting current directory");
        }
    }
}
