use crate::selector;
use eyre::{eyre, Result, WrapErr};
use log::{debug, warn};
use std::path::Path;
use std::process::Command;

/// Runs a jj command and returns the trimmed stdout.
fn run_jj_command(workdir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("jj")
        .args(args)
        .current_dir(workdir)
        .output()
        .wrap_err_with(|| format!("Failed to execute jj command: {:?}", args))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "jj command {:?} failed: {}
{}",
            args, output.status, stderr
        );
        return Err(eyre!(
            "jj command {:?} failed: {}
{}",
            args,
            output.status,
            stderr
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Checks if the jj working copy is clean.
fn is_working_copy_clean(workdir: &Path) -> Result<bool> {
    let status = run_jj_command(workdir, &["status"]).wrap_err("Failed to run jj status")?;
    Ok(!status.contains("Working copy changes"))
}

/// Gets the first bookmark name at the given revision, if any.
fn get_bookmark_at(
    workdir: &Path,
    rev: &str,
) -> Result<Option<String>> {
    let bookmarks = run_jj_command(workdir, &["bookmark", "list", "-r", rev])
        .wrap_err_with(|| format!("Failed to list bookmarks for rev: {}", rev))?;
    Ok(bookmarks
        .lines()
        .next()
        .map(|line| line.split(":").next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Gets the short commit ID of the given revision.
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

/// Gets a list of recent bookmark names.
fn get_recent_bookmarks(workdir: &Path) -> Result<Vec<String>> {
    let output = run_jj_command(workdir, &["bookmark", "list"]).wrap_err("Failed to list bookmarks")?;
    Ok(output
        .lines()
        .map(|line| line.split(":").next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// Generates a tag to be used for the build output directory.
pub fn generate_tag(
    workdir: &Path,
    explicit_tag: Option<String>,
) -> Result<String> {
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

    let prompt = "Select tag for build output";
    let selection_result = selector::select_app_path(prompt, options, None);
    debug!("tag_generator selection_result: {:?}", selection_result);

    let selection = selection_result.wrap_err("Failed to select tag")?;

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
