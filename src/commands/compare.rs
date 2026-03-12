use clap::Parser;
use log::{debug, error, info};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::selector::{self, BuildArtifacts};

/// Arguments for the `compare` subcommand.
#[derive(Parser, Debug)]
pub struct CompareArgs {
    /// Baseline build file path (e.g., out/branch-builds/tag/app).
    ///
    /// If omitted, an interactive selection will be shown.
    pub from_file: Option<String>,

    /// Comparison build file path (e.g., out/branch-builds/tag/app).
    ///
    /// If omitted, an interactive selection will be shown based on the application
    /// selected for `from_file`.
    pub to_file: Option<String>,

    /// Extra arguments to pass to the underlying diff script.
    ///
    /// These arguments are passed after `--` to this subcommand.
    #[arg(last = true)]
    pub extra_diff_args: Vec<String>,
}

/// Holds the fully resolved paths for the two artifacts to be compared.
struct ResolvedCompareArgs {
    from_path: PathBuf,
    to_path: PathBuf,
}

/// Parses an artifact path string into tag and application path components.
///
/// Expected format: "out/branch-builds/<tag>/<app_path>"
/// Returns `Some((tag, app_path))` on success, `None` otherwise.
fn parse_artifact_path(path_str: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path_str.splitn(4, '/').collect();
    if parts.len() == 4 && parts[0] == "out" && parts[1] == "branch-builds" {
        Some((parts[2].to_string(), parts[3].to_string())) // (tag, app_path)
    } else {
        None
    }
}

/// Resolves the `from_file` and `to_file` arguments, prompting the user interactively if necessary.
///
/// If file paths are not provided in `args`, this function discovers available build artifacts
/// and uses `dialoguer` to guide the user through selecting the application and tags to compare.
fn resolve_compare_args(
    args: &CompareArgs,
    workdir: &Path,
) -> Result<ResolvedCompareArgs, Box<dyn std::error::Error>> {
    let artifacts = BuildArtifacts::find(workdir)?;

    let from_file = match &args.from_file {
        Some(f) => f.clone(),
        None => {
            let app_paths = artifacts.get_app_paths();
            if app_paths.is_empty() {
                return Err("No build artifacts found.".into());
            }
            let app_path_options: Vec<String> = artifacts
                .apps
                .iter()
                .map(|(app_path, tags)| format!("{}  (Tags: {})", app_path, tags.join(", ")))
                .collect();
            let selection_index =
                dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Select application")
                    .items(&app_path_options)
                    .default(0)
                    .interact()?;
            let selected_app_path = artifacts.get_app_paths()[selection_index].clone();
            let tags = artifacts.get_tags_for_app(&selected_app_path).unwrap();
            let selected_tag = selector::select_tag("Select BASELINE tag", tags)?;
            selector::build_path(&selected_tag, &selected_app_path)
        }
    };

    let (from_tag, from_app_path) = parse_artifact_path(&from_file)
        .ok_or_else(|| format!("Invalid from_file path format: {}", from_file))?;

    let to_file = match &args.to_file {
        Some(f) => f.clone(),
        None => {
            let tags = artifacts.get_tags_for_app(&from_app_path).unwrap();
            let other_tags: Vec<&String> = tags.iter().filter(|t| t != &&from_tag).collect();
            if other_tags.is_empty() {
                return Err(
                    format!("No other tags found for application: {}", from_app_path).into(),
                );
            }
            let selected_tag = selector::select_string("Select COMPARISON tag", &other_tags)?;
            selector::build_path(&selected_tag, &from_app_path)
        }
    };

    Ok(ResolvedCompareArgs {
        from_path: workdir.join(&from_file),
        to_path: workdir.join(&to_file),
    })
}

/// Executes the size difference script to compare the two artifact files.
///
/// Uses `uv run` to execute the Python script `scripts/tools/binary_elf_size_diff.py`,
/// passing the file paths and any extra arguments.
fn run_diff(
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    extra_args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if !from_path.exists() {
        error!("From file not found: {}", from_path.display());
        return Err(format!("From file not found: {}", from_path.display()).into());
    }
    if !to_path.exists() {
        error!("To file not found: {}", to_path.display());
        return Err(format!("To file not found: {}", to_path.display()).into());
    }

    info!(
        "Comparing {} and {}",
        from_path.display(),
        to_path.display()
    );

    let mut command = Command::new("uv");
    command.args(["run", "scripts/tools/binary_elf_size_diff.py"]);

    if extra_args.is_empty() {
        command.args(["--output", "table"]);
    } else {
        command.args(extra_args);
    }

    command.arg(to_path);
    command.arg(from_path);

    debug!("Running command: {:?}", command);
    command.current_dir(workdir);
    let status = command
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        error!("Diff command failed with status: {}", status);
        return Err(format!("Diff command failed with status: {}", status).into());
    }
    Ok(())
}

/// Handles the logic for the `compare` subcommand.
///
/// Resolves the arguments (potentially interactively) and then runs the diff process.
pub fn handle_compare(
    args: &CompareArgs,
    workdir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolved_args = resolve_compare_args(args, workdir)?;
    run_diff(
        &resolved_args.from_path,
        &resolved_args.to_path,
        workdir,
        &args.extra_diff_args,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_artifact_path_valid() {
        assert_eq!(
            parse_artifact_path("out/branch-builds/v1.0/app/test.elf"),
            Some(("v1.0".to_string(), "app/test.elf".to_string()))
        );
        assert_eq!(
            parse_artifact_path("out/branch-builds/my-tag/other_app"),
            Some(("my-tag".to_string(), "other_app".to_string()))
        );
    }

    #[test]
    fn test_parse_artifact_path_invalid() {
        assert_eq!(parse_artifact_path(""), None);
        assert_eq!(parse_artifact_path("out/branch-builds/tag"), None); // Too short
        assert_eq!(parse_artifact_path("foo/bar/tag/app"), None); // Wrong prefix
        assert_eq!(parse_artifact_path("out/branch-builds/t1/t2/t3"), Some(("t1".to_string(), "t2/t3".to_string()))); // Deeper path
    }
}
