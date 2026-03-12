use dialoguer::{Select, theme::ColorfulTheme};
use std::collections::BTreeMap;
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
    /// It scans for files ending in ".elf", ".bin", or files with no extension
    /// within subdirectories structured as `<tag>/<app_path>`.
    pub fn find(workdir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let builds_dir = workdir.join("out/branch-builds");
        let mut apps: BTreeMap<String, Vec<String>> = BTreeMap::new();

        if !builds_dir.exists() {
            return Ok(BuildArtifacts { apps });
        }

        for entry in WalkDir::new(&builds_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file()
                && let Some(filename) = entry.path().file_name().and_then(|n| n.to_str())
                && (!filename.contains('.')
                    || filename.ends_with(".elf")
                    || filename.ends_with(".bin"))
            {
                let relative_path = entry.path().strip_prefix(&builds_dir)?;
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

/// Presents an interactive selection prompt to the user to choose from a list of strings.
///
/// Uses `dialoguer` to display the `prompt` and the list of `items`.
pub fn select_string(
    prompt: &str,
    items: &[&String],
) -> Result<String, Box<dyn std::error::Error>> {
    if items.is_empty() {
        return Err("No items to select from.".into());
    }
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .interact()?;
    Ok(items[selection].clone())
}

/// Presents an interactive selection prompt for choosing a tag from a list.
///
/// Similar to `select_string`, but specialized for `Vec<String>` of tags.
pub fn select_tag(prompt: &str, tags: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    if tags.is_empty() {
        return Err("No tags to select from.".into());
    }
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(tags)
        .default(0)
        .interact()?;
    Ok(tags[selection].clone())
}

/// Constructs the relative path to an artifact given a tag and application path.
///
/// Format: "out/branch-builds/<tag>/<app_path>"
pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("out/branch-builds/{}/{}", tag, app_path)
}
