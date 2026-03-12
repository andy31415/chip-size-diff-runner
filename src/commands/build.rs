use clap::Parser;
use log::{debug, error, info};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;

use crate::tag_generator;

/// Arguments for the `build` subcommand.
#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Application to build (e.g., linux-x64-all-clusters-app).
    pub application: String,

    /// Optional tag/bookmark name to associate with the build.
    ///
    /// If not provided, the tool will attempt to infer a suitable tag
    /// based on the jj repository state or prompt the user.
    #[arg(short, long)]
    pub tag: Option<String>,
}

/// Handles the logic for the `build` subcommand.
///
/// Determines the build tag/bookmark, creates the output directory, and orchestrates the build execution.
pub fn handle_build(args: &BuildArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tag = tag_generator::generate_tag(workdir, args.tag.clone())?;

    info!("Building application: {}", args.application);
    info!("Using tag: {}", tag);

    let relative_output_dir = format!("out/branch-builds/{}", tag);
    let output_dir = workdir.join(&relative_output_dir);
    std::fs::create_dir_all(&output_dir)?;

    info!("Output directory: {}", output_dir.display());

    execute_build(
        &args.application,
        &relative_output_dir,
        &output_dir,
        workdir,
    )?;

    Ok(())
}

/// Executes the application build command.
///
/// Dispatches to either a local bash execution or a podman container based on the application name prefix.
fn execute_build(
    application: &str,
    relative_output_dir: &str,
    output_dir: &Path,
    workdir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
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
        .status()?;

    if !status.success() {
        error!("Build command failed with status: {}", status);
        return Err(format!("Build command failed with status: {}", status).into());
    }

    info!("Artifacts in: {}", output_dir.display());
    Ok(())
}
