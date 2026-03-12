use clap::Parser;
use log::{debug, error, info};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Application to build
    pub application: String,

    /// Optional tag for the build
    #[arg(short, long)]
    pub tag: Option<String>,
}

fn get_jj_tag(workdir: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("jj")
        .arg("tag")
        .arg("list")
        .arg("-r")
        .arg("@-")
        .current_dir(workdir)
        .output()?;

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout)?;
        let tag = stdout
            .lines()
            .next()
            .and_then(|line| line.split(':').next())
            .map(str::trim);
        Ok(tag.map(String::from))
    } else {
        Ok(None)
    }
}

pub fn handle_build(args: &BuildArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tag = match &args.tag {
        Some(t) => t.clone(),
        None => get_jj_tag(workdir)?
            .ok_or("Error: No --tag provided and no jj tag found at @- in this repository")?,
    };

    info!("Building application: {}", args.application);
    info!("Using tag: {}", tag);

    let output_dir = workdir.join(format!("out/branch-builds/{}", tag));
    std::fs::create_dir_all(&output_dir)?;

    info!("Output directory: {}", output_dir.display());

    execute_build(&args.application, &output_dir, workdir)?;

    Ok(())
}

fn execute_build(
    application: &str,
    output_dir: &Path,
    workdir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_dir_str = output_dir.to_string_lossy();
    let build_command = format!(
        "source ./scripts/activate.sh >/dev/null && ./scripts/build/build_examples.py --log-level info --target '{}' build --copy-artifacts-to {}",
        application, output_dir_str
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
