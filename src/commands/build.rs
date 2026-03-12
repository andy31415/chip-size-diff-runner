use clap::Parser;
use eyre::{Result, WrapErr, eyre};
use log::{debug, error, info};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;

use crate::defaults;
use crate::selector;
use crate::tag_generator;

const HARDCODED_TARGETS: &[&str] = &[
    "linux-x64-all-clusters-app",
    "linux-x64-chip-tool",
    "linux-x64-all-devices",
    "efr32-brd4187c-lock-no-version",
    "stm32-stm32wb5mm-dk-light",
    "qpg-qpg6200-light",
    "ti-cc13x4_26x4-lock-ftd",
];

/// Arguments for the `build` subcommand.
#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Application to build (e.g., linux-x64-all-clusters-app).
    ///
    /// If omitted, an interactive selection will be shown combining recent builds,
    /// targets discovered from out/branch-builds, and a built-in list.
    pub application: Option<String>,

    /// Optional tag/bookmark name to associate with the build.
    ///
    /// If not provided, the tool will attempt to infer a suitable tag
    /// based on the jj repository state or prompt the user.
    #[arg(short, long)]
    pub tag: Option<String>,
}

/// Scans `out/branch-builds/<tag>/<target>/` and returns unique first-level subdirectory names,
/// which correspond to build target names used in previous builds.
fn discover_targets_from_builds(workdir: &Path) -> Vec<String> {
    let builds_dir = workdir.join("out/branch-builds");
    let mut targets = BTreeSet::new();

    let Ok(branch_entries) = fs::read_dir(&builds_dir) else {
        return vec![];
    };

    for branch_entry in branch_entries.flatten() {
        if !branch_entry
            .file_type()
            .map(|t| t.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(target_entries) = fs::read_dir(branch_entry.path()) else {
            continue;
        };
        for target_entry in target_entries.flatten() {
            if target_entry
                .file_type()
                .map(|t| t.is_dir())
                .unwrap_or(false)
                && let Some(name) = target_entry.file_name().to_str()
            {
                targets.insert(name.to_string());
            }
        }
    }

    targets.into_iter().collect()
}

/// Builds the ordered, deduplicated list of candidate targets:
/// recents first (preserving order), then discovered, then hardcoded.
fn build_candidate_list(recent: &[String], discovered: Vec<String>) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut candidates: Vec<String> = Vec::new();

    for app in recent {
        if seen.insert(app.clone()) {
            candidates.push(app.clone());
        }
    }
    for app in discovered {
        if seen.insert(app.clone()) {
            candidates.push(app);
        }
    }
    for &app in HARDCODED_TARGETS {
        if seen.insert(app.to_string()) {
            candidates.push(app.to_string());
        }
    }

    candidates
}

/// Resolves the application name: uses the CLI argument if provided, otherwise
/// presents an interactive fuzzy-find selection.
fn resolve_application(
    args: &BuildArgs,
    workdir: &Path,
    defaults: &defaults::ComparisonDefaults,
) -> Result<String> {
    if let Some(app) = &args.application {
        return Ok(app.clone());
    }

    let discovered = discover_targets_from_builds(workdir);
    debug!("Discovered targets from builds dir: {:?}", discovered);

    let candidates = build_candidate_list(&defaults.recent_applications, discovered);

    let default_item = defaults.recent_applications.first().cloned();
    selector::select_app_path("Select build target", candidates, default_item)
        .wrap_err("Failed to select build target")
}

/// Handles the logic for the `build` subcommand.
///
/// Determines the build tag/bookmark, creates the output directory, and orchestrates the build execution.
pub fn handle_build(args: &BuildArgs, workdir: &Path) -> Result<()> {
    let mut defaults = defaults::load_defaults().wrap_err("Failed to load defaults")?;

    let application = resolve_application(args, workdir, &defaults)?;

    let tag_result = tag_generator::generate_tag(workdir, args.tag.clone());
    debug!("handle_build tag_result: {:?}", tag_result);

    let tag = tag_result.wrap_err("Failed to generate tag")?;

    info!("Building application: {}", application);
    info!("Using tag: {}", tag);

    let relative_output_dir = format!("out/branch-builds/{}", tag);
    let output_dir = workdir.join(&relative_output_dir);
    std::fs::create_dir_all(&output_dir).wrap_err_with(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    info!("Output directory: {}", output_dir.display());

    execute_build(&application, &relative_output_dir, &output_dir, workdir)
        .wrap_err("Failed to execute build")?;

    defaults.add_recent_application(&application);
    defaults::save_defaults(&defaults).wrap_err("Failed to save defaults")?;

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
        error!("Build command failed with status: {}", status);
        return Err(eyre!("Build command failed with status: {}", status));
    }

    info!("Artifacts in: {}", output_dir.display());
    Ok(())
}
