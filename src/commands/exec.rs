use anyhow::{bail, Result};
use std::path::Path;

use crate::docker::exec::exec_in_container;
use crate::docker::DockerManager;
use crate::orchestrator::state::ProjectState;

pub async fn run(config_path: Option<&Path>, docker_name: &str, command: Vec<String>) -> Result<()> {
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => crate::config::resolve::resolve_config(None)?,
    };

    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);

    let state = ProjectState::load(&state_dir).ok_or_else(|| {
        anyhow::anyhow!("no running project state found -- is the project running?")
    })?;

    let docker_state = state.docker.get(docker_name).ok_or_else(|| {
        anyhow::anyhow!(
            "docker '{}' not found (available: {:?})",
            docker_name,
            state.docker.keys().collect::<Vec<_>>()
        )
    })?;

    if command.is_empty() {
        bail!("no command specified");
    }

    let mgr = DockerManager::new(state.slug.clone()).await?;
    let (exit_code, output) =
        exec_in_container(mgr.docker(), &docker_state.container_id, command).await?;

    print!("{}", output);

    if exit_code != 0 {
        std::process::exit(exit_code as i32);
    }

    Ok(())
}
