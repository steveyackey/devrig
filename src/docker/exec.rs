use anyhow::{bail, Context, Result};
use bollard::exec::{StartExecOptions, StartExecResults};
use bollard::models::ExecConfig;
use bollard::Docker;
use futures_util::StreamExt;

use crate::config::model::DockerConfig;

/// Execute a command in a container and return (exit_code, combined_output).
pub async fn exec_in_container(
    docker: &Docker,
    container_id: &str,
    cmd: Vec<String>,
) -> Result<(i64, String)> {
    let config = ExecConfig {
        cmd: Some(cmd),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    let exec = docker
        .create_exec(container_id, config)
        .await
        .context("creating exec instance")?;

    let mut output = String::new();
    let start_options = StartExecOptions {
        detach: false,
        ..Default::default()
    };
    if let StartExecResults::Attached {
        output: mut stream, ..
    } = docker
        .start_exec(&exec.id, Some(start_options))
        .await
        .context("starting exec")?
    {
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(log) => output.push_str(&log.to_string()),
                Err(e) => tracing::warn!("exec stream error: {}", e),
            }
        }
    }

    let inspect = docker
        .inspect_exec(&exec.id)
        .await
        .context("inspecting exec")?;
    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok((exit_code, output))
}

/// Run init scripts for a docker service.
pub async fn run_init_scripts(
    docker: &Docker,
    container_id: &str,
    docker_name: &str,
    docker_config: &DockerConfig,
) -> Result<()> {
    for (i, script) in docker_config.init.iter().enumerate() {
        tracing::debug!(
            docker = %docker_name,
            "running init script {}/{}",
            i + 1,
            docker_config.init.len()
        );

        let cmd = if docker_config.image.starts_with("postgres") {
            let user = docker_config
                .env
                .get("POSTGRES_USER")
                .map(|s| s.as_str())
                .unwrap_or("postgres");
            vec![
                "psql".to_string(),
                "-U".to_string(),
                user.to_string(),
                "-c".to_string(),
                script.clone(),
            ]
        } else {
            vec!["sh".to_string(), "-c".to_string(), script.clone()]
        };

        let (exit_code, output) = exec_in_container(docker, container_id, cmd).await?;

        if !output.trim().is_empty() {
            tracing::debug!(docker = %docker_name, "init output: {}", output.trim());
        }

        if exit_code != 0 {
            bail!(
                "init script {}/{} for '{}' failed with exit code {} — output: {}",
                i + 1,
                docker_config.init.len(),
                docker_name,
                exit_code,
                output.trim()
            );
        }
    }

    Ok(())
}

/// Run a command-based ready check via docker exec.
pub async fn exec_ready_check(
    docker: &Docker,
    container_id: &str,
    command: &str,
    expect: Option<&str>,
) -> Result<()> {
    let cmd = vec!["sh".to_string(), "-c".to_string(), command.to_string()];
    let (exit_code, output) = exec_in_container(docker, container_id, cmd).await?;

    if exit_code != 0 {
        bail!("command '{}' exited with code {}", command, exit_code);
    }

    if let Some(expected) = expect {
        if !output.contains(expected) {
            bail!(
                "command '{}' output did not contain '{}' (got: '{}')",
                command,
                expected,
                output.trim()
            );
        }
    }

    Ok(())
}
