use crate::ui::fuzzy::{self, SelectItem};
use eyre::{Result, WrapErr, eyre};
use log::{debug, warn};
use std::path::Path;
use std::process::Command;

/// Runs a jj command in `workdir` and returns trimmed stdout, or an error with stderr.
fn run_jj_command(workdir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("jj")
        .args(args)
        .current_dir(workdir)
        .output()
        .wrap_err_with(|| format!("Failed to execute jj command: {:?}", args))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!(
            "jj command {:?} failed: {}\n{}",
            args,
            output.status,
            stderr
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Returns `true` if the jj working copy has no uncommitted changes.
///
/// Used to decide whether `@-` (the last committed change) is a reliable source
/// for the build tag, or if we need to prompt the user instead.
fn is_working_copy_clean(workdir: &Path) -> Result<bool> {
    let status = run_jj_command(workdir, &["status"]).wrap_err("Failed to run jj status")?;
    Ok(!status.contains("Working copy changes"))
}

/// Extracts the bookmark name from a single line of `jj bookmark list` output.
///
/// `jj bookmark list` emits lines like `"my-feature: abc123 ..."`.
/// Returns `None` for empty or malformed lines.
#[must_use]
fn parse_bookmark_name(line: &str) -> Option<String> {
    let name = line.split(':').next().unwrap_or("").trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

/// Returns the first bookmark at `rev`, if any.
///
/// `@-` is the typical revision used here — it's the parent of the working copy,
/// i.e. the last clean committed change, which is the right tag for a build.
fn get_bookmark_at(workdir: &Path, rev: &str) -> Result<Option<String>> {
    let output = run_jj_command(workdir, &["bookmark", "list", "-r", rev])
        .wrap_err_with(|| format!("Failed to list bookmarks for rev: {}", rev))?;
    Ok(output.lines().next().and_then(parse_bookmark_name))
}

/// Returns the short change ID of the given revision (e.g. `"kxqv"`).
fn get_short_commit_id(workdir: &Path, rev: &str) -> Result<String> {
    run_jj_command(
        workdir,
        &[
            "log",
            "-r",
            rev,
            "--no-graph",
            "--template",
            "change_id.shortest()",
        ],
    )
    .wrap_err_with(|| format!("Failed to get short commit ID for rev: {}", rev))
}

/// Returns all bookmark names in the repository, for use as manual tag options.
fn list_bookmarks(workdir: &Path) -> Result<Vec<String>> {
    let output =
        run_jj_command(workdir, &["bookmark", "list"]).wrap_err("Failed to list bookmarks")?;
    Ok(output.lines().filter_map(parse_bookmark_name).collect())
}

/// The choices offered to the user when a tag cannot be determined automatically.
enum TagOption {
    CommitId(String),
    Bookmark(String),
    Custom,
}

impl SelectItem for TagOption {
    fn display_text(&self) -> String {
        match self {
            Self::CommitId(id) => format!("Commit ID: {}", id),
            Self::Bookmark(name) => format!("Bookmark: {}", name),
            Self::Custom => "Enter custom tag…".to_string(),
        }
    }
}

/// Builds the ordered list of tag options for the interactive prompt.
///
/// Order: commit ID first, custom entry second, then all bookmarks.
fn build_tag_options(commit_id: String, bookmarks: Vec<String>) -> Vec<TagOption> {
    let mut options = vec![TagOption::CommitId(commit_id), TagOption::Custom];
    for bookmark in bookmarks {
        options.push(TagOption::Bookmark(bookmark));
    }
    options
}

/// Resolves the tag (output directory name) to use for a build.
///
/// Priority:
/// 1. `explicit_tag` if provided via `--tag`.
/// 2. The bookmark at `@-` if the working copy is clean.
/// 3. Interactive prompt: current commit ID, bookmarks, or custom entry.
pub fn generate_tag(workdir: &Path, explicit_tag: Option<String>) -> Result<String> {
    if let Some(tag) = explicit_tag {
        return Ok(tag);
    }

    if is_working_copy_clean(workdir)? {
        if let Some(bookmark) = get_bookmark_at(workdir, "@-")? {
            debug!("Using bookmark at @-: {}", bookmark);
            return Ok(bookmark);
        }
        debug!("Working copy clean, but no bookmark found at @-");
    }

    let commit_id = get_short_commit_id(workdir, "@")?;
    let bookmarks = match list_bookmarks(workdir) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to get bookmarks: {}", e);
            vec![]
        }
    };

    let options = build_tag_options(commit_id.clone(), bookmarks);
    let selection = fuzzy::select("Select tag for build output", options, None)
        .wrap_err("Failed to select tag")?;
    debug!("tag_generator selection: {:?}", selection.display_text());

    match selection {
        TagOption::CommitId(_) => Ok(format!("jj-{}", commit_id)),
        TagOption::Bookmark(name) => Ok(name),
        TagOption::Custom => {
            let tag = dialoguer::Input::new()
                .with_prompt("Enter custom tag")
                .interact_text()
                .wrap_err("Failed to read custom tag")?;
            Ok(tag)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bookmark_name_normal() {
        assert_eq!(
            parse_bookmark_name("my-feature: abc123 (remote)"),
            Some("my-feature".to_string())
        );
    }

    #[test]
    fn test_parse_bookmark_name_no_colon() {
        assert_eq!(
            parse_bookmark_name("just-a-name"),
            Some("just-a-name".to_string())
        );
    }

    #[test]
    fn test_parse_bookmark_name_empty() {
        assert_eq!(parse_bookmark_name(""), None);
        assert_eq!(parse_bookmark_name("   "), None);
    }

    #[test]
    fn test_parse_bookmark_name_trims_whitespace() {
        assert_eq!(
            parse_bookmark_name("  spaced-name  : rest"),
            Some("spaced-name".to_string())
        );
    }

    #[test]
    fn test_build_tag_options_ordering() {
        let opts = build_tag_options(
            "abc123".to_string(),
            vec!["feature-a".to_string(), "feature-b".to_string()],
        );
        assert!(matches!(opts[0], TagOption::CommitId(_)));
        assert!(matches!(opts[1], TagOption::Custom));
        assert!(matches!(opts[2], TagOption::Bookmark(_)));
        assert!(matches!(opts[3], TagOption::Bookmark(_)));
    }

    #[test]
    fn test_build_tag_options_no_bookmarks() {
        let opts = build_tag_options("abc123".to_string(), vec![]);
        assert_eq!(opts.len(), 2);
        assert!(matches!(opts[0], TagOption::CommitId(_)));
        assert!(matches!(opts[1], TagOption::Custom));
    }

    #[test]
    fn test_tag_option_display_text() {
        assert_eq!(
            TagOption::CommitId("kxqv".to_string()).display_text(),
            "Commit ID: kxqv"
        );
        assert_eq!(
            TagOption::Bookmark("main".to_string()).display_text(),
            "Bookmark: main"
        );
        assert_eq!(TagOption::Custom.display_text(), "Enter custom tag…");
    }
}
