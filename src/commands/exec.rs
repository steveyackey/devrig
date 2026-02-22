use anyhow::{bail, Result};
use std::path::Path;

use crate::infra::exec::exec_in_container;
use crate::infra::InfraManager;
use crate::orchestrator::state::ProjectState;

pub async fn run(config_path: Option<&Path>, infra_name: &str, command: Vec<String>) -> Result<()> {
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => crate::config::resolve::resolve_config(None)?,
    };

    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);

    let state = ProjectState::load(&state_dir).ok_or_else(|| {
        anyhow::anyhow!("no running project state found -- is the project running?")
    })?;

    let infra_state = state.infra.get(infra_name).ok_or_else(|| {
        anyhow::anyhow!(
            "infra '{}' not found (available: {:?})",
            infra_name,
            state.infra.keys().collect::<Vec<_>>()
        )
    })?;

    if command.is_empty() {
        bail!("no command specified");
    }

    let mgr = InfraManager::new(state.slug.clone()).await?;
    let (exit_code, output) =
        exec_in_container(mgr.docker(), &infra_state.container_id, command).await?;

    print!("{}", output);

    if exit_code != 0 {
        std::process::exit(exit_code as i32);
    }

    Ok(())
}
