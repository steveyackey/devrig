use std::process::Command;

pub fn run() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine install directory"))?;

    let updater_name = if cfg!(windows) {
        "devrig-update.exe"
    } else {
        "devrig-update"
    };
    let updater = exe_dir.join(updater_name);

    if !updater.exists() {
        anyhow::bail!(
            "updater not found at {}\n\n\
             The updater is included when installing via the shell/powershell installer:\n  \
             curl --proto '=https' --tlsv1.2 -LsSf https://github.com/steveyackey/devrig/releases/latest/download/devrig-installer.sh | sh\n\n\
             Other install methods can update by re-running the install command.",
            updater.display()
        );
    }

    let status = Command::new(&updater).status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
