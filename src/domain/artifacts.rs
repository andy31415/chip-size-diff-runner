use crate::ui::fuzzy::SelectItem;
use chrono::{DateTime, Local};
use eyre::{Result, WrapErr};
use goblin::elf::Elf;
use log::debug;
use owo_colors::OwoColorize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// A build artifact application path with its associated tags.
pub struct AppItem {
    pub path: String,
    pub tag_names: Vec<String>,
    pub column_width: usize,
}

impl SelectItem for AppItem {
    fn display_text(&self) -> String {
        format!(
            "{:<width$} (Tags: {})",
            self.path,
            self.tag_names.join(", "),
            width = self.column_width
        )
    }

    fn skim_text(&self) -> String {
        format!(
            "{:<width$} {}",
            self.path,
            format!("(Tags: {})", self.tag_names.join(", ")).green(),
            width = self.column_width
        )
    }
}

/// A build tag with its ELF modification time.
pub struct TagItem {
    pub name: String,
    pub modified: SystemTime,
    pub column_width: usize,
}

impl SelectItem for TagItem {
    fn display_text(&self) -> String {
        let dt: DateTime<Local> = self.modified.into();
        format!(
            "{:<width$}  ({})",
            self.name,
            dt.format("%Y-%m-%d %H:%M:%S"),
            width = self.column_width
        )
    }

    fn skim_text(&self) -> String {
        let dt: DateTime<Local> = self.modified.into();
        format!(
            "{:<width$}  {}",
            self.name,
            format!("({})", dt.format("%Y-%m-%d %H:%M:%S")).green(),
            width = self.column_width
        )
    }
}

/// Creates a `Vec<TagItem>` from raw `(name, mtime)` pairs, computing the column
/// width over the provided set so timestamps align correctly across all items.
pub fn create_tag_items(entries: &[(String, SystemTime)]) -> Vec<TagItem> {
    let width = entries
        .iter()
        .map(|(t, _)| t.len())
        .max()
        .unwrap_or(0)
        .min(30);
    entries
        .iter()
        .map(|(name, modified)| TagItem {
            name: name.clone(),
            modified: *modified,
            column_width: width,
        })
        .collect()
}

/// The collection of ELF build artifacts found under `out/branch-builds/`.
pub struct BuildArtifacts {
    /// app_path → Vec<(tag_name, modified_time)>, sorted newest-first.
    pub apps: BTreeMap<String, Vec<(String, SystemTime)>>,
}

impl BuildArtifacts {
    /// Walks `out/branch-builds/` in the workdir, identifies ELF files, records
    /// their modification time, and sorts each app's tags newest-first.
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
                                if let Some(existing) = entries.iter_mut().find(|(t, _)| t == &tag)
                                {
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

        for entries in apps.values_mut() {
            entries.sort_by(|a, b| b.1.cmp(&a.1));
        }

        Ok(BuildArtifacts { apps })
    }

    /// Returns all apps as `AppItem`s with the path column aligned (capped at 80 chars).
    pub fn app_items(&self) -> Vec<AppItem> {
        let width = self.apps.keys().map(|p| p.len()).max().unwrap_or(0).min(80);
        self.apps
            .iter()
            .map(|(path, entries)| AppItem {
                path: path.clone(),
                tag_names: entries.iter().map(|(t, _)| t.clone()).collect(),
                column_width: width,
            })
            .collect()
    }

    /// Returns tag items for the given app, sorted newest-first, with aligned column.
    pub fn tag_items_for_app(&self, app_path: &str) -> Option<Vec<TagItem>> {
        self.apps.get(app_path).map(|e| create_tag_items(e))
    }
}

/// Constructs the relative path to an artifact: `"out/branch-builds/<tag>/<app_path>"`.
pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("out/branch-builds/{}/{}", tag, app_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::fuzzy::strip_ansi_codes;

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
    fn test_tag_item_display_alignment() {
        let entries = vec![
            ("short".to_string(), SystemTime::UNIX_EPOCH),
            ("a-longer-tag-name".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let items = create_tag_items(&entries);
        let w0 = items[0].display_text().find("  (").unwrap();
        let w1 = items[1].display_text().find("  (").unwrap();
        assert_eq!(w0, w1);
    }

    #[test]
    fn test_tag_item_skim_text_alignment() {
        let entries = vec![
            ("short".to_string(), SystemTime::UNIX_EPOCH),
            ("a-longer-tag-name".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let items = create_tag_items(&entries);
        let w0 = items[0].skim_text().find("  \x1b[").unwrap();
        let w1 = items[1].skim_text().find("  \x1b[").unwrap();
        assert_eq!(w0, w1);
    }

    #[test]
    fn test_app_items_alignment() {
        let mut apps = BTreeMap::new();
        apps.insert(
            "short/path".to_string(),
            vec![("tag1".to_string(), SystemTime::UNIX_EPOCH)],
        );
        apps.insert(
            "a/much/longer/application/path".to_string(),
            vec![("tag2".to_string(), SystemTime::UNIX_EPOCH)],
        );
        let artifacts = BuildArtifacts { apps };
        let items = artifacts.app_items();
        let positions: Vec<usize> = items
            .iter()
            .map(|i| i.display_text().find(" (Tags:").unwrap())
            .collect();
        assert!(positions.iter().all(|&p| p == positions[0]));
    }

    #[test]
    fn test_display_text_matches_stripped_skim_text() {
        let entries = vec![("my-tag".to_string(), SystemTime::UNIX_EPOCH)];
        let items = create_tag_items(&entries);
        let item = &items[0];
        assert_eq!(item.display_text(), strip_ansi_codes(&item.skim_text()));
    }
}
