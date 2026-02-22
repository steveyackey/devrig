use anyhow::Result;
use owo_colors::OwoColorize;
use std::path::Path;

use crate::config;
use crate::config::resolve::resolve_config;
use crate::config::validate::validate;

pub fn run(config_file: Option<&Path>) -> Result<()> {
    let config_path = resolve_config(config_file)?;

    let (config, source) = config::load_config(&config_path)?;

    let filename = config_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "devrig.toml".to_string());

    match validate(&config, &source, &filename) {
        Ok(()) => {
            let svc_count = config.services.len();
            let infra_count = config.infra.len();
            println!(
                "  {} {} is valid ({} services, {} infra)",
                "\u{2713}".green(),
                filename,
                svc_count,
                infra_count,
            );
            Ok(())
        }
        Err(errors) => {
            for err in errors {
                let report: miette::Report = err.into();
                eprintln!("{:?}", report);
            }
            std::process::exit(1);
        }
    }
}
