use crate::domain::artifacts::BUILDS_PATH_PREFIX;
use crate::domain::vcs;
use crate::persistence::SessionState;
use crate::runner::build_engine;
use crate::ui::fuzzy;
use clap::Parser;
use eyre::{Result, WrapErr};
use log::{debug, info};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

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

/// Scans `out/branch-builds/<tag>/<target>/` and returns unique first-level
/// subdirectory names, which correspond to build target names used in previous builds.
fn discover_targets_from_builds(workdir: &Path) -> Vec<String> {
    let builds_dir = workdir.join(BUILDS_PATH_PREFIX);
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
/// recents first (preserving order), then discovered, then default_targets.
#[must_use]
fn build_candidate_list(
    recent: &[String],
    discovered: Vec<String>,
    default_targets: &[String],
) -> Vec<String> {
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
    for app in default_targets {
        if seen.insert(app.clone()) {
            candidates.push(app.clone());
        }
    }

    candidates
}

/// Resolves the application name: uses the CLI argument if provided, otherwise
/// presents an interactive fuzzy-find selection.
fn resolve_application(args: &BuildArgs, workdir: &Path, session: &SessionState) -> Result<String> {
    if let Some(app) = &args.application {
        return Ok(app.clone());
    }

    let discovered = discover_targets_from_builds(workdir);
    debug!("Discovered targets from builds dir: {:?}", discovered);

    let candidates = build_candidate_list(
        &session.recent_applications,
        discovered,
        &session.default_targets,
    );

    // If there are recents, the most recent is already at index 0 (from build_candidate_list).
    let default_index = if session.recent_applications.is_empty() {
        None
    } else {
        Some(0)
    };
    fuzzy::select("Select build target", candidates, default_index)
        .wrap_err("Failed to select build target")
}

/// Handles the logic for the `build` subcommand.
///
/// Determines the build tag/bookmark, creates the output directory, and orchestrates
/// the build execution. Accepts the session loaded by the caller to avoid a double load.
pub fn handle_build(args: &BuildArgs, workdir: &Path, mut session: SessionState) -> Result<()> {
    let application = resolve_application(args, workdir, &session)?;

    let tag_result = vcs::generate_tag(workdir, args.tag.clone());
    debug!("handle_build tag_result: {:?}", tag_result);

    let tag = tag_result.wrap_err("Failed to generate tag")?;

    info!("Building application: {}", application);
    info!("Using tag: {}", tag);

    let relative_output_dir = format!("{}/{}", BUILDS_PATH_PREFIX, tag);
    let output_dir = workdir.join(&relative_output_dir);
    std::fs::create_dir_all(&output_dir).wrap_err_with(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    info!("Output directory: {}", output_dir.display());

    build_engine::execute_build(&application, &relative_output_dir, &output_dir, workdir)
        .wrap_err("Failed to execute build")?;

    session.add_recent_application(&application);
    session.save().wrap_err("Failed to save session state")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_list_ordering() {
        let recent = vec!["recent-a".to_string(), "recent-b".to_string()];
        let discovered = vec!["discovered-x".to_string()];
        let defaults = vec!["default-1".to_string()];
        let result = build_candidate_list(&recent, discovered, &defaults);
        assert_eq!(result[0], "recent-a");
        assert_eq!(result[1], "recent-b");
        assert_eq!(result[2], "discovered-x");
        assert_eq!(result[3], "default-1");
    }

    #[test]
    fn test_candidate_list_deduplication() {
        let recent = vec!["app-1".to_string()];
        let discovered = vec!["app-1".to_string()];
        let defaults = vec!["app-1".to_string()];
        let result = build_candidate_list(&recent, discovered, &defaults);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "app-1");
    }

    #[test]
    fn test_candidate_list_empty_recents_falls_back_to_defaults() {
        let defaults = vec!["default-1".to_string()];
        let result = build_candidate_list(&[], vec![], &defaults);
        assert_eq!(result[0], "default-1");
    }
}
