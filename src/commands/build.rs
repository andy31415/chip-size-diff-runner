use clap::Parser;
use log::{debug, error, info};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;

/// Arguments for the `build` subcommand.
#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Application to build (e.g., linux-x64-all-clusters-app).
    pub application: String,

    /// Optional tag to associate with the build.
    ///
    /// If not provided, the tool will attempt to infer the current `jj` tag.
    /// The build artifacts will be stored in a directory named after this tag.
    #[arg(short, long)]
    pub tag: Option<String>,
}

/// Retrieves the latest `jj` tag from the current repository checkout.
///
/// Executes `jj tag list -r @-` to find the tag associated with the parent commit.
fn get_jj_tag(workdir: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    debug!("Attempting to get jj tag from workdir: {}", workdir.display());
    let command = Command::new("jj")
        .arg("tag")
        .arg("list")
        .arg("-r")
        .arg("@-")
        .current_dir(workdir)
        .output();

    match command {
        Ok(output) => {
            let stdout = str::from_utf8(&output.stdout).unwrap_or("[non-utf8 stdout]");
            let stderr = str::from_utf8(&output.stderr).unwrap_or("[non-utf8 stderr]");
            debug!("`jj tag list -r @-` status: {}", output.status);
            debug!("`jj tag list -r @-` stdout:
{}", stdout);
            debug!("`jj tag list -r @-` stderr:
{}", stderr);

            if output.status.success() {
                let tag = stdout
                    .lines()
                    .next()
                    .and_then(|line| line.split(':').next())
                    .map(str::trim);
                debug!("Parsed tag: {:?}", tag);
                Ok(tag.map(String::from))
            } else {
                error!("jj tag command failed");
                Ok(None)
            }
        }
        Err(e) => {
            error!("Failed to execute jj command: {}", e);
            Err(e.into())
        }
    }
}

/// Handles the logic for the `build` subcommand.
///
/// Determines the build tag, creates the output directory, and orchestrates the build execution.
pub fn handle_build(args: &BuildArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tag = match &args.tag {
        Some(t) => t.clone(),
        None => get_jj_tag(workdir)?
            .ok_or("Error: No --tag provided and no jj tag found at @- in this repository")?,
    };

    info!("Building application: {}", args.application);
    info!("Using tag: {}", tag);

    let relative_output_dir = format!("out/branch-builds/{}", tag);
    let output_dir = workdir.join(&relative_output_dir);
    std::fs::create_dir_all(&output_dir)?;

    info!("Output directory: {}", output_dir.display());

    execute_build(&args.application, &relative_output_dir, &output_dir, workdir)?;

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
