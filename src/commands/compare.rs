use crate::domain::artifacts::{
    AppItem, BUILDS_PATH_PREFIX, BuildArtifacts, build_path, create_tag_items,
};
use crate::persistence::SessionState;
use crate::runner::diff_engine::{self, ViewerTool};
use crate::ui::fuzzy::{self, SelectItem};
use clap::Parser;
use eyre::{Result, WrapErr, eyre};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

    /// Viewer tool to pipe CSV output to.
    ///
    /// Options: default, vd, visidata, csvlens, custom:<name>
    ///
    /// "default" auto-detects: prefers `vd`, then `csvlens`, then plain table output.
    #[arg(long, default_value = "default")]
    pub viewer: String,

    /// Diff engine to use for comparison.
    ///
    /// Options: script, nm, native, goblin
    #[arg(long, default_value = "native")]
    pub diff_engine: String,

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
/// Expected format: `"<BUILDS_PATH_PREFIX>/<tag>/<app_path>"`
/// Returns `Some((tag, app_path))` on success, `None` otherwise.
fn parse_artifact_path(path_str: &str) -> Option<(String, String)> {
    let rest = path_str.strip_prefix(&format!("{}/", BUILDS_PATH_PREFIX))?;
    let mut parts = rest.splitn(2, '/');
    let tag = parts.next()?.to_string();
    let app = parts.next()?.to_string();
    Some((tag, app))
}

/// Returns all entries for an app excluding the given tag.
///
/// Used when selecting the comparison target to prevent comparing a build with itself.
#[must_use]
fn filter_other_entries(
    entries: &[(String, SystemTime)],
    exclude_tag: &str,
) -> Vec<(String, SystemTime)> {
    entries
        .iter()
        .filter(|(name, _)| name != exclude_tag)
        .cloned()
        .collect()
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

/// Items presented when picking an application or returning the last compare.
enum ComparePromptItem {
    LastCompare {
        from_file: String,
        to_file: String,
        label: String,
    },
    App(AppItem),
}

impl SelectItem for ComparePromptItem {
    fn display_text(&self) -> String {
        match self {
            Self::LastCompare { label, .. } => format!("[Last Compare] {}", label),
            Self::App(app) => app.display_text(),
        }
    }

    fn skim_text(&self) -> String {
        match self {
            Self::LastCompare { label, .. } => {
                format!("{}", format!("[Last Compare] {}", label).cyan().bold())
            }
            Self::App(app) => app.skim_text(),
        }
    }
}

/// Prompts the user to select a tag for the given app.
///
/// Shared by both the baseline and comparison selection steps. Builds tag
/// items from `entries`, optionally pre-selects the tag recorded in
/// `session_file` (if it matches `app_path`), and returns the full
/// `build_path` for the chosen tag.
fn select_tag(
    prompt: &str,
    entries: &[(String, SystemTime)],
    session_file: Option<&str>,
    app_path: &str,
) -> Result<String> {
    let tag_items = create_tag_items(entries);

    let default_index = session_file
        .and_then(parse_artifact_path)
        .filter(|(_, app)| app == app_path)
        .and_then(|(tag, _)| tag_items.iter().position(|i| i.name == tag));

    let selected = fuzzy::select(prompt, tag_items, default_index)
        .wrap_err_with(|| format!("Failed to select tag for: {}", prompt))?;

    Ok(build_path(&selected.name, app_path))
}

/// Resolves the `from_file` and `to_file` arguments, prompting the user interactively if necessary.
fn resolve_compare_args(
    args: &CompareArgs,
    workdir: &Path,
    session: &SessionState,
) -> Result<ResolvedCompareArgs> {
    let artifacts = BuildArtifacts::find(workdir).wrap_err("Failed to find build artifacts")?;

    let mut from_file_str = args
        .from_file
        .as_ref()
        .map(|f| normalize_path_str(f, workdir));
    let mut to_file_str = args
        .to_file
        .as_ref()
        .map(|f| normalize_path_str(f, workdir));

    if from_file_str.is_none() {
        // ── Select application ────────────────────────────────────────────
        let app_items = artifacts.app_items();
        if app_items.is_empty() {
            return Err(eyre!("No build artifacts found."));
        }

        let mut prompt_items = Vec::new();
        let mut has_last_compare = false;

        if to_file_str.is_none()
            && let (Some(from), Some(to)) =
                (session.from_file.as_deref(), session.to_file.as_deref())
        {
            let label = match (parse_artifact_path(from), parse_artifact_path(to)) {
                (Some((from_tag, from_app)), Some((to_tag, to_app))) => {
                    if from_app == to_app {
                        format!("{} ({} vs {})", from_app, from_tag, to_tag)
                    } else {
                        format!("{} ({}) vs {} ({})", from_app, from_tag, to_app, to_tag)
                    }
                }
                _ => format!("{} vs {}", from, to),
            };
            prompt_items.push(ComparePromptItem::LastCompare {
                from_file: from.to_string(),
                to_file: to.to_string(),
                label,
            });
            has_last_compare = true;
        }

        let default_app_index = session
            .from_file
            .as_deref()
            .and_then(parse_artifact_path)
            .and_then(|(_, app_path)| app_items.iter().position(|i| i.path == app_path))
            .map(|idx| if has_last_compare { idx + 1 } else { idx });

        for app in app_items {
            prompt_items.push(ComparePromptItem::App(app));
        }

        let selected = fuzzy::select(
            if has_last_compare {
                "Select application (or Last Compare)"
            } else {
                "Select application"
            },
            prompt_items,
            if has_last_compare {
                Some(0)
            } else {
                default_app_index
            },
        )
        .wrap_err("Failed to select application")?;

        match selected {
            ComparePromptItem::LastCompare {
                from_file, to_file, ..
            } => {
                from_file_str = Some(from_file);
                to_file_str = Some(to_file);
            }
            ComparePromptItem::App(selected_app) => {
                let entries = artifacts
                    .apps
                    .get(&selected_app.path)
                    .ok_or_else(|| eyre!("No tags found for app: {}", selected_app.path))?;

                from_file_str = Some(select_tag(
                    "Select BASELINE tag",
                    entries,
                    session.from_file.as_deref(),
                    &selected_app.path,
                )?);
            }
        }
    }

    let from_file_str = from_file_str.unwrap();

    let (from_tag, from_app_path) = parse_artifact_path(&from_file_str).ok_or_else(|| {
        eyre!(
            "Invalid from_file path format: {}. Expected: out/branch-builds/<tag>/<app_path>",
            from_file_str
        )
    })?;

    let to_file_str = match to_file_str {
        Some(f) => f,
        None => {
            // Exclude the baseline tag so the user can't compare a build with itself.
            let all_entries = artifacts
                .apps
                .get(&from_app_path)
                .ok_or_else(|| eyre!("No tags found for app: {}", from_app_path))?;
            let other_entries = filter_other_entries(all_entries, &from_tag);
            if other_entries.is_empty() {
                return Err(eyre!(
                    "No other tags found for application: {}",
                    from_app_path
                ));
            }

            select_tag(
                "Select COMPARISON tag",
                &other_entries,
                session.to_file.as_deref(),
                &from_app_path,
            )?
        }
    };

    Ok(ResolvedCompareArgs {
        from_path: workdir.join(&from_file_str),
        to_path: workdir.join(&to_file_str),
    })
}

/// Handles the logic for the `compare` subcommand.
///
/// Accepts the session loaded by the caller to avoid a double load.
pub fn handle_compare(args: &CompareArgs, workdir: &Path, mut session: SessionState) -> Result<()> {
    let viewer: ViewerTool = args.viewer.parse().wrap_err("Invalid --viewer value")?;
    let diff_engine: diff_engine::DiffEngine = args
        .diff_engine
        .parse()
        .wrap_err("Invalid --diff-engine value")?;

    let resolved_args = resolve_compare_args(args, workdir, &session)
        .wrap_err("Failed to resolve compare arguments")?;

    diff_engine::run_diff(
        &resolved_args.from_path,
        &resolved_args.to_path,
        workdir,
        &diff_engine,
        &args.extra_diff_args,
        &viewer,
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

    // Confirms that parse_artifact_path correctly inverts build_path.
    // These two functions must stay in sync; this test links them explicitly.
    #[test]
    fn test_parse_artifact_path_roundtrips_build_path() {
        use crate::domain::artifacts::build_path;
        assert_eq!(
            parse_artifact_path(&build_path("my-tag", "sub/app.elf")),
            Some(("my-tag".to_string(), "sub/app.elf".to_string()))
        );
        assert_eq!(
            parse_artifact_path(&build_path("v1.0", "app")),
            Some(("v1.0".to_string(), "app".to_string()))
        );
    }

    #[test]
    fn test_filter_other_entries_excludes_tag() {
        let entries = vec![
            ("tag-a".to_string(), SystemTime::UNIX_EPOCH),
            ("tag-b".to_string(), SystemTime::UNIX_EPOCH),
            ("tag-c".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let result = filter_other_entries(&entries, "tag-b");
        let names: Vec<&str> = result.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["tag-a", "tag-c"]);
    }

    #[test]
    fn test_filter_other_entries_only_tag_returns_empty() {
        let entries = vec![("only-tag".to_string(), SystemTime::UNIX_EPOCH)];
        assert!(filter_other_entries(&entries, "only-tag").is_empty());
    }

    #[test]
    fn test_filter_other_entries_missing_tag_returns_all() {
        let entries = vec![
            ("tag-a".to_string(), SystemTime::UNIX_EPOCH),
            ("tag-b".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let result = filter_other_entries(&entries, "tag-x");
        assert_eq!(result.len(), 2);
    }
}
