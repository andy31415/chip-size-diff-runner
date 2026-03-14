use crate::domain::artifacts::{BuildArtifacts, build_path, create_tag_items};
use crate::persistence::SessionState;
use crate::runner::diff_engine;
use crate::ui::fuzzy;
use clap::Parser;
use eyre::{Result, WrapErr, eyre};
use std::path::{Path, PathBuf};

/// Arguments for the `compare` subcommand.
#[derive(Parser, Debug)]
pub struct CompareArgs {
    /// Baseline build file path (e.g., out/branch-builds/tag/app).
    ///
    /// Can be an absolute path within the workdir, or relative to the workdir.
    /// If omitted, an interactive selection will be shown.
    pub from_file: Option<String>,

    /// Comparison build file path (e.g., out/branch-builds/tag/app).
    ///
    /// Can be an absolute path within the workdir, or relative to the workdir.
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

/// Normalizes a given path string. If the path is absolute, it attempts to strip
/// the workdir prefix to make it relative. Otherwise, returns it as is.
fn normalize_path_str(path_str: &str, workdir: &Path) -> String {
    let path = PathBuf::from(path_str);
    if path.is_absolute() {
        path.strip_prefix(workdir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path_str.to_string())
    } else {
        path_str.to_string()
    }
}

/// Resolves the `from_file` and `to_file` arguments, prompting the user interactively if necessary.
fn resolve_compare_args(
    args: &CompareArgs,
    workdir: &Path,
    session: &SessionState,
) -> Result<ResolvedCompareArgs> {
    let artifacts = BuildArtifacts::find(workdir).wrap_err("Failed to find build artifacts")?;

    let from_file_str = match &args.from_file {
        Some(f) => normalize_path_str(f, workdir),
        None => {
            // ── Select application ────────────────────────────────────────────
            let app_items = artifacts.app_items();
            if app_items.is_empty() {
                return Err(eyre!("No build artifacts found."));
            }

            let default_app_index = session
                .from_file
                .as_deref()
                .and_then(parse_artifact_path)
                .and_then(|(_, app_path)| app_items.iter().position(|i| i.path == app_path));

            let selected_app = fuzzy::select("Select application", app_items, default_app_index)
                .wrap_err("Failed to select application")?;

            // ── Select baseline tag ───────────────────────────────────────────
            let tag_items = artifacts
                .tag_items_for_app(&selected_app.path)
                .ok_or_else(|| eyre!("Failed to get tags for app: {}", selected_app.path))?;

            let default_tag_index = session
                .from_file
                .as_deref()
                .and_then(parse_artifact_path)
                .filter(|(_, app)| app == &selected_app.path)
                .and_then(|(tag, _)| tag_items.iter().position(|i| i.name == tag));

            let selected_tag = fuzzy::select("Select BASELINE tag", tag_items, default_tag_index)
                .wrap_err("Failed to select baseline tag")?;

            build_path(&selected_tag.name, &selected_app.path)
        }
    };

    let (from_tag, from_app_path) = parse_artifact_path(&from_file_str).ok_or_else(|| {
        eyre!(
            "Invalid from_file path format: {}. Expected: out/branch-builds/<tag>/<app_path>",
            from_file_str
        )
    })?;

    let to_file_str = match &args.to_file {
        Some(f) => normalize_path_str(f, workdir),
        None => {
            // ── Select comparison tag ─────────────────────────────────────────
            // Build tag items excluding the already-chosen baseline tag, so the
            // user can't accidentally compare a build with itself. Column width
            // is recomputed over the filtered set for correct alignment.
            let all_entries = artifacts
                .apps
                .get(&from_app_path)
                .ok_or_else(|| eyre!("Failed to get tags for app: {}", from_app_path))?;
            let other_entries: Vec<_> = all_entries
                .iter()
                .filter(|(name, _)| name != &from_tag)
                .cloned()
                .collect();
            if other_entries.is_empty() {
                return Err(eyre!(
                    "No other tags found for application: {}",
                    from_app_path
                ));
            }

            let tag_items = create_tag_items(&other_entries);

            let default_tag_index = session
                .to_file
                .as_deref()
                .and_then(parse_artifact_path)
                .filter(|(_, app)| app == &from_app_path)
                .and_then(|(tag, _)| tag_items.iter().position(|i| i.name == tag));

            let selected_tag = fuzzy::select("Select COMPARISON tag", tag_items, default_tag_index)
                .wrap_err("Failed to select comparison tag")?;

            build_path(&selected_tag.name, &from_app_path)
        }
    };

    Ok(ResolvedCompareArgs {
        from_path: workdir.join(&from_file_str),
        to_path: workdir.join(&to_file_str),
    })
}

/// Handles the logic for the `compare` subcommand.
pub fn handle_compare(args: &CompareArgs, workdir: &Path) -> Result<()> {
    let mut session = SessionState::load().wrap_err("Failed to load session state")?;
    let resolved_args = resolve_compare_args(args, workdir, &session)
        .wrap_err("Failed to resolve compare arguments")?;

    diff_engine::run_diff(
        &resolved_args.from_path,
        &resolved_args.to_path,
        workdir,
        &args.extra_diff_args,
    )
    .wrap_err("Failed to run diff")?;

    session.from_file = Some(normalize_path_str(
        &resolved_args.from_path.to_string_lossy(),
        workdir,
    ));
    session.to_file = Some(normalize_path_str(
        &resolved_args.to_path.to_string_lossy(),
        workdir,
    ));
    session.save().wrap_err("Failed to save session state")?;

    Ok(())
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
        assert_eq!(parse_artifact_path("out/branch-builds/tag"), None);
        assert_eq!(parse_artifact_path("foo/bar/tag/app"), None);
        assert_eq!(
            parse_artifact_path("out/branch-builds/t1/t2/t3"),
            Some(("t1".to_string(), "t2/t3".to_string()))
        );
    }

    #[test]
    fn test_normalize_path_str() {
        let workdir = PathBuf::from("/home/user/project");
        assert_eq!(
            normalize_path_str("out/branch-builds/tag/app", &workdir),
            "out/branch-builds/tag/app"
        );
        assert_eq!(
            normalize_path_str("/home/user/project/out/branch-builds/tag/app", &workdir),
            "out/branch-builds/tag/app"
        );
        assert_eq!(
            normalize_path_str("/other/path/out/branch-builds/tag/app", &workdir),
            "/other/path/out/branch-builds/tag/app"
        );
        assert_eq!(
            normalize_path_str("relative/path", &workdir),
            "relative/path"
        );
    }
}
