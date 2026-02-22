use anyhow::{bail, Result};
use std::path::Path;

use crate::orchestrator::state::ProjectState;

pub fn run(config_path: Option<&Path>, infra_name: &str) -> Result<()> {
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => crate::config::resolve::resolve_config(None)?,
    };

    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);

    let mut state = ProjectState::load(&state_dir).ok_or_else(|| {
        anyhow::anyhow!("no project state found -- has the project been started?")
    })?;

    if !state.reset_init(infra_name) {
        bail!(
            "infra '{}' not found in state (available: {:?})",
            infra_name,
            state.infra.keys().collect::<Vec<_>>()
        );
    }

    state.save(&state_dir)?;
    println!(
        "Reset init flag for '{}'. Init scripts will run on next start.",
        infra_name
    );

    Ok(())
}
