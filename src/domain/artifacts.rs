use crate::ui::fuzzy::SelectItem;
use chrono::{DateTime, Local};
use eyre::Result;
use goblin::elf::Elf;
use log::debug;
use owo_colors::OwoColorize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

/// A build artifact application path with its associated tags.
#[derive(Default)]
pub struct AppItem {
    pub path: String,
    pub tag_names: BTreeSet<String>, // sorted alphabetically
    pub column_width: usize,
}

impl AppItem {
    fn csv_tags(&self) -> String {
        self.tag_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl SelectItem for AppItem {
    fn display_text(&self) -> String {
        format!(
            "{:<width$} (Tags: {})",
            self.path,
            self.csv_tags(),
            width = self.column_width
        )
    }

    fn skim_text(&self) -> String {
        format!(
            "{:<width$} {}",
            self.path,
            format!("(Tags: {})", self.csv_tags()).green(),
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

/// Root-relative prefix for all branch build outputs.
///
/// This is the single source of truth for the path format shared by
/// `build_path()` and `parse_artifact_path()` in the compare command.
pub const BUILDS_PATH_PREFIX: &str = "out/branch-builds";

/// Tries to extract artifact metadata from a path. Returns `None` for any file
/// that should be skipped (non-ELF, unreadable, unexpected path structure).
fn extract_artifact(path: &Path, builds_dir: &Path) -> Option<(String, String, SystemTime)> {
    let buffer = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            debug!("Error reading file {}: {}", path.display(), e);
            return None;
        }
    };

    if Elf::parse(&buffer).is_err() {
        debug!("Skipping non-ELF file: {}", path.display());
        return None;
    }

    let relative_path = path.strip_prefix(builds_dir).ok().or_else(|| {
        debug!("Failed to strip prefix from {}", path.display());
        None
    })?;
    let components: Vec<&str> = relative_path
        .iter()
        .map(|s| s.to_str().unwrap_or(""))
        .collect();

    if components.len() <= 1 {
        debug!(
            "Skipping file with unexpected path structure: {}",
            path.display()
        );
        return None;
    }

    let tag = components[0].to_string();
    let app_path = PathBuf::from_iter(&components[1..])
        .to_string_lossy()
        .to_string();
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    debug!("Found ELF artifact: {}", path.display());
    Some((tag, app_path, mtime))
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
        let builds_dir = workdir.join(BUILDS_PATH_PREFIX);
        let mut apps: BTreeMap<String, Vec<(String, SystemTime)>> = BTreeMap::new();

        if !builds_dir.exists() {
            return Ok(BuildArtifacts { apps });
        }

        for (tag, app_path, mtime) in WalkDir::new(&builds_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| extract_artifact(e.path(), &builds_dir))
        {
            let entries = apps.entry(app_path).or_default();
            if let Some(existing) = entries.iter_mut().find(|(t, _)| t == &tag) {
                if mtime > existing.1 {
                    existing.1 = mtime;
                }
            } else {
                entries.push((tag, mtime));
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

/// Constructs the relative path to an artifact.
///
/// # Examples
///
/// ```
/// use branch_diff::domain::artifacts::build_path;
/// assert_eq!(build_path("v1.0", "chip-tool"), "out/branch-builds/v1.0/chip-tool");
/// assert_eq!(build_path("my-tag", "sub/dir/app"), "out/branch-builds/my-tag/sub/dir/app");
/// ```
#[must_use]
pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("{}/{}/{}", BUILDS_PATH_PREFIX, tag, app_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::fuzzy::strip_ansi_codes;
    use std::fs;
    use tempfile::TempDir;

    // Returns a minimal valid ELF64 header (64 bytes).
    fn minimal_elf64() -> Vec<u8> {
        let mut b = vec![0u8; 64];
        b[0..4].copy_from_slice(b"\x7fELF");
        b[4] = 2; // ELFCLASS64
        b[5] = 1; // ELFDATA2LSB
        b[6] = 1; // EV_CURRENT
        b[16] = 2; // e_type: ET_EXEC
        b[18] = 0x3e; // e_machine: EM_X86_64
        b[20] = 1; // e_version
        b[52] = 64; // e_ehsize
        b[54] = 56; // e_phentsize
        b[58] = 64; // e_shentsize
        b
    }

    fn write_elf(dir: &TempDir, relative: &str) -> std::path::PathBuf {
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, minimal_elf64()).unwrap();
        path
    }

    #[test]
    fn test_find_missing_builds_dir_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let artifacts = BuildArtifacts::find(tmp.path()).unwrap();
        assert!(artifacts.apps.is_empty());
    }

    #[test]
    fn test_find_non_elf_file_is_ignored() {
        let tmp = TempDir::new().unwrap();
        let build_dir = tmp.path().join(BUILDS_PATH_PREFIX).join("tag1");
        fs::create_dir_all(&build_dir).unwrap();
        fs::write(build_dir.join("not_elf.bin"), b"this is not an elf").unwrap();

        let artifacts = BuildArtifacts::find(tmp.path()).unwrap();
        assert!(artifacts.apps.is_empty());
    }

    #[test]
    fn test_find_discovers_elf_and_groups_by_app() {
        let tmp = TempDir::new().unwrap();
        write_elf(&tmp, &format!("{}/tag1/subdir/app.elf", BUILDS_PATH_PREFIX));
        write_elf(&tmp, &format!("{}/tag2/subdir/app.elf", BUILDS_PATH_PREFIX));

        let artifacts = BuildArtifacts::find(tmp.path()).unwrap();
        assert_eq!(artifacts.apps.len(), 1);
        let tags: Vec<&str> = artifacts.apps["subdir/app.elf"]
            .iter()
            .map(|(t, _)| t.as_str())
            .collect();
        assert!(tags.contains(&"tag1"));
        assert!(tags.contains(&"tag2"));
    }

    #[test]
    fn test_find_multiple_apps_separated() {
        let tmp = TempDir::new().unwrap();
        write_elf(&tmp, &format!("{}/tag1/app-a/binary", BUILDS_PATH_PREFIX));
        write_elf(&tmp, &format!("{}/tag1/app-b/binary", BUILDS_PATH_PREFIX));

        let artifacts = BuildArtifacts::find(tmp.path()).unwrap();
        assert_eq!(artifacts.apps.len(), 2);
        assert!(artifacts.apps.contains_key("app-a/binary"));
        assert!(artifacts.apps.contains_key("app-b/binary"));
    }

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
        // build_path must always start with the shared prefix constant.
        assert!(build_path("t", "a").starts_with(BUILDS_PATH_PREFIX));
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

    #[test]
    fn test_csv_tags() {
        let mut item = AppItem::default();

        assert_eq!(item.csv_tags(), "");

        item.tag_names = BTreeSet::from_iter(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(item.csv_tags(), "a, b, c");

        item.tag_names = BTreeSet::from_iter(vec!["test".into()]);
        assert_eq!(item.csv_tags(), "test");

        // we alpha-sort the tag names
        item.tag_names = BTreeSet::from_iter(vec!["test".into(), "another".into()]);
        assert_eq!(item.csv_tags(), "another, test");
    }
}
