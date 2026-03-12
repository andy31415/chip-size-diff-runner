use chrono::{DateTime, Local};
use eyre::{Result, WrapErr, eyre};
use goblin::elf::Elf;
use log::debug;
use skim::prelude::{Skim, SkimItemReader, SkimOptionsBuilder};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// Represents the collection of build artifacts found in the working directory.
pub struct BuildArtifacts {
    /// A map where keys are application paths (relative to the tag directory)
    /// and values are `(tag, modified_time)` pairs sorted newest-first.
    pub apps: BTreeMap<String, Vec<(String, SystemTime)>>,
}

impl BuildArtifacts {
    /// Finds and catalogs all build artifacts within the workdir's "out/branch-builds" directory.
    ///
    /// Scans for ELF binaries, records their modification time, and sorts each app's tags
    /// by modification time descending (newest first).
    pub fn find(workdir: &Path) -> Result<Self> {
        let builds_dir = workdir.join("out/branch-builds");
        let mut apps: BTreeMap<String, Vec<(String, SystemTime)>> = BTreeMap::new();

        if !builds_dir.exists() {
            return Ok(BuildArtifacts { apps });
        }

        for entry in WalkDir::new(&builds_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let path = entry.path();
                match fs::read(path) {
                    Ok(buffer) => {
                        if Elf::parse(&buffer).is_ok() {
                            let relative_path = path
                                .strip_prefix(&builds_dir)
                                .wrap_err("Failed to strip prefix from path")?;
                            let components: Vec<&str> = relative_path
                                .iter()
                                .map(|s| s.to_str().unwrap_or(""))
                                .collect();

                            if components.len() > 1 {
                                let tag = components[0].to_string();
                                let app_path = PathBuf::from_iter(&components[1..])
                                    .to_string_lossy()
                                    .to_string();
                                let mtime = fs::metadata(path)
                                    .and_then(|m| m.modified())
                                    .unwrap_or(SystemTime::UNIX_EPOCH);
                                let entries = apps.entry(app_path).or_default();
                                // Replace existing entry for this tag or push new one
                                if let Some(existing) = entries.iter_mut().find(|(t, _)| t == &tag)
                                {
                                    // Keep the newest mtime for this tag
                                    if mtime > existing.1 {
                                        existing.1 = mtime;
                                    }
                                } else {
                                    entries.push((tag, mtime));
                                }
                                debug!("Found ELF artifact: {}", path.display());
                            } else {
                                debug!(
                                    "Skipping file with unexpected path structure: {}",
                                    path.display()
                                );
                            }
                        } else {
                            debug!("Skipping non-ELF file: {}", path.display());
                        }
                    }
                    Err(e) => {
                        debug!("Error reading file {}: {}", path.display(), e);
                    }
                }
            }
        }

        // Sort each app's tags newest-first
        for entries in apps.values_mut() {
            entries.sort_by(|a, b| b.1.cmp(&a.1));
        }

        Ok(BuildArtifacts { apps })
    }

    /// Returns a vector of unique application paths found.
    pub fn get_app_paths(&self) -> Vec<&String> {
        self.apps.keys().collect()
    }

    /// Returns the list of tag names for the given app, sorted newest-first.
    pub fn get_tags_for_app(&self, app_path: &str) -> Option<Vec<String>> {
        self.apps
            .get(app_path)
            .map(|entries| entries.iter().map(|(tag, _)| tag.clone()).collect())
    }

    /// Returns display strings (`"tag  (YYYY-MM-DD HH:MM)"`) for the given app, sorted newest-first.
    pub fn get_tag_display_items_for_app(&self, app_path: &str) -> Option<Vec<String>> {
        self.apps.get(app_path).map(|entries| {
            entries
                .iter()
                .map(|(tag, mtime)| format_tag_display(tag, *mtime))
                .collect()
        })
    }

    /// Returns the display string for a specific `(app_path, tag)` pair, used to set skim defaults.
    pub fn tag_to_display_item(&self, app_path: &str, tag: &str) -> Option<String> {
        self.apps
            .get(app_path)?
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(t, mtime)| format_tag_display(t, *mtime))
    }
}

/// Formats a tag name and its ELF modification time into a display string for skim.
pub fn format_tag_display(tag: &str, mtime: SystemTime) -> String {
    let dt: DateTime<Local> = mtime.into();
    format!("{}  ({})", tag, dt.format("%Y-%m-%d %H:%M"))
}

/// Strips the date suffix from a skim display string to recover the raw tag name.
///
/// Expects the format produced by `format_tag_display`: `"tag  (YYYY-MM-DD HH:MM)"`.
pub fn parse_tag_from_display(display: &str) -> String {
    display.split("  (").next().unwrap_or(display).to_string()
}

/// Presents an interactive fuzzy finder to the user to choose from a list of strings.
fn fuzzy_select(
    prompt: &str,
    mut items: Vec<String>,
    default_item: Option<String>,
) -> Result<String> {
    if items.is_empty() {
        return Err(eyre!("No items to select from."));
    }

    if let Some(def_item) = default_item
        && let Some(index) = items.iter().position(|item| item == &def_item)
    {
        let item = items.remove(index);
        items.insert(0, item);
    }

    let options = SkimOptionsBuilder::default()
        .prompt(format!("{}: ", prompt))
        .build()
        .wrap_err("Failed to build Skim options")?;

    let item_string = items.join("\n");
    let item_reader = SkimItemReader::default();
    let skim_items = item_reader.of_bufread(Cursor::new(item_string));

    match Skim::run_with(options, Some(skim_items)) {
        Ok(out) => {
            debug!("Skim output: {:?}", out);
            if out.is_abort {
                debug!("Skim selection aborted by user (e.g., ESC)");
                Err(eyre!("Selection cancelled by user."))
            } else {
                let selected_items = out.selected_items;
                if selected_items.is_empty() {
                    debug!("Skim selection empty, but not an abort");
                    Err(eyre!("No selection made."))
                } else {
                    Ok(selected_items[0].output().to_string())
                }
            }
        }
        Err(e) => {
            debug!("Skim returned error: {} - treated as cancellation", e);
            Err(eyre!("Selection process failed or was cancelled."))
        }
    }
}

/// Presents an interactive fuzzy finder for choosing a tag from display items.
///
/// Items should be formatted display strings (e.g. from `get_tag_display_items_for_app`).
/// Use `parse_tag_from_display` on the result to recover the raw tag name.
pub fn select_tag(
    prompt: &str,
    items: Vec<String>,
    default_item: Option<String>,
) -> Result<String> {
    fuzzy_select(prompt, items, default_item)
}

/// Presents an interactive fuzzy finder for choosing an application path.
pub fn select_app_path(
    prompt: &str,
    app_paths: Vec<String>,
    default_item: Option<String>,
) -> Result<String> {
    fuzzy_select(prompt, app_paths, default_item)
}

/// Constructs the relative path to an artifact given a tag and application path.
///
/// Format: "out/branch-builds/<tag>/<app_path>"
pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("out/branch-builds/{}/{}", tag, app_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_path() {
        assert_eq!(
            build_path("v1.0", "app/test.elf"),
            "out/branch-builds/v1.0/app/test.elf"
        );
        assert_eq!(
            build_path("my-tag", "other_app"),
            "out/branch-builds/my-tag/other_app"
        );
    }

    #[test]
    fn test_parse_tag_from_display() {
        assert_eq!(
            parse_tag_from_display("my-branch  (2024-01-15 14:23)"),
            "my-branch"
        );
        assert_eq!(parse_tag_from_display("plain-tag"), "plain-tag");
    }
}
