use crate::selector;
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
fn get_recent_bookmarks(workdir: &Path) -> Result<Vec<String>> {
    let output =
        run_jj_command(workdir, &["bookmark", "list"]).wrap_err("Failed to list bookmarks")?;
    Ok(output.lines().filter_map(parse_bookmark_name).collect())
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

    let current_commit_id = get_short_commit_id(workdir, "@")?;
    let mut options = vec![
        format!("Use current commit ID: {}", current_commit_id),
        "Enter custom tag".to_string(),
    ];

    match get_recent_bookmarks(workdir) {
        Ok(bookmarks) => {
            for bookmark in bookmarks {
                options.push(format!("Use bookmark: {}", bookmark));
            }
        }
        Err(e) => warn!("Failed to get recent bookmarks: {}", e),
    }

    let selection = selector::select("Select tag for build output", options, None)
        .wrap_err("Failed to select tag")?;
    debug!("tag_generator selection: {:?}", selection);

    if selection.starts_with("Use current commit ID: ") {
        Ok(format!("jj-{}", current_commit_id))
    } else if selection == "Enter custom tag" {
        // TODO: Prompt for custom tag input
        Err(eyre!("Custom tag input not yet implemented"))
    } else if selection.starts_with("Use bookmark: ") {
        Ok(selection.replace("Use bookmark: ", ""))
    } else {
        Err(eyre!("Unexpected selection: {}", selection))
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
        // Lines without a colon are unexpected but should not panic.
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
}
