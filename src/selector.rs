use eyre::{Result, WrapErr, eyre};
use goblin::elf::Elf;
use log::debug;
use skim::prelude::{Skim, SkimItemReader, SkimOptionsBuilder};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Represents the collection of build artifacts found in the working directory.
pub struct BuildArtifacts {
    /// A map where keys are application paths (relative to the tag directory)
    /// and values are sorted lists of tags under which this application artifact exists.
    pub apps: BTreeMap<String, Vec<String>>,
}

impl BuildArtifacts {
    /// Finds and catalogs all build artifacts within the workdir's "out/branch-builds" directory.
    ///
    /// It scans for files and verifies if they are ELF binaries by parsing their headers.
    /// Files are expected to be within subdirectories structured as `<tag>/<app_path>`.
    pub fn find(workdir: &Path) -> Result<Self> {
        let builds_dir = workdir.join("out/branch-builds");
        let mut apps: BTreeMap<String, Vec<String>> = BTreeMap::new();

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
                                apps.entry(app_path).or_default().push(tag);
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

        // Sort and unique tags for each app
        for tags in apps.values_mut() {
            tags.sort();
            tags.dedup();
        }

        Ok(BuildArtifacts { apps })
    }

    /// Returns a vector of unique application paths found.
    pub fn get_app_paths(&self) -> Vec<&String> {
        self.apps.keys().collect()
    }

    /// Returns the list of tags available for a given application path.
    pub fn get_tags_for_app(&self, app_path: &str) -> Option<&Vec<String>> {
        self.apps.get(app_path)
    }
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
        .prompt(prompt.to_string())
        .build()
        .wrap_err("Failed to build Skim options")?;

    let item_string = items.join(
        "
",
    );
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

/// Presents an interactive fuzzy finder for choosing from a list of string slices.
pub fn select_string(
    prompt: &str,
    items: &[&String],
    default_item: Option<String>,
) -> Result<String> {
    let owned_items: Vec<String> = items.iter().map(|s| (*s).clone()).collect();
    fuzzy_select(prompt, owned_items, default_item)
}

/// Presents an interactive fuzzy finder for choosing a tag from a list.
pub fn select_tag(prompt: &str, tags: &[String], default_item: Option<String>) -> Result<String> {
    let owned_tags: Vec<String> = tags.to_vec();
    fuzzy_select(prompt, owned_tags, default_item)
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
}
