use eyre::{Result, WrapErr, eyre};
use log::{debug, info};
use std::path::Path;
use std::process::{Command, Stdio};

/// Executes the application build command.
///
/// Dispatches to either a local bash execution or a podman container based on the application name prefix.
pub fn execute_build(
    application: &str,
    relative_output_dir: &str,
    output_dir: &Path,
    workdir: &Path,
) -> Result<()> {
    let build_command = format!(
        "source ./scripts/activate.sh >/dev/null && ./scripts/build/build_examples.py --log-level info --target '{}' build --copy-artifacts-to {}",
        application, relative_output_dir
    );

    let mut command;
    if application.starts_with("linux-x64-") {
        info!("Building on HOST...");
        command = Command::new("bash");
        command.arg("-c").arg(build_command);
    } else {
        info!("Building via PODMAN...");
        command = Command::new("podman");
        command.args([
            "exec",
            "-w",
            "/workspace",
            "bld_vscode",
            "/bin/bash",
            "-c",
            &build_command,
        ]);
    }

    debug!("Executing build command: {:?}", command);
    command.current_dir(workdir);
    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .wrap_err("Failed to execute build command")?;

    if !status.success() {
        return Err(eyre!("Build command failed with status: {}", status));
    }

    info!("Artifacts in: {}", output_dir.display());
    Ok(())
}
