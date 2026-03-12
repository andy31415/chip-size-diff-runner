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

// ── SelectItem trait ─────────────────────────────────────────────────────────

/// Items that can be presented in a skim fuzzy-find prompt.
///
/// Implement this trait to make a type selectable. `select()` returns the
/// original typed value — no string parsing required after selection.
pub trait SelectItem: Send + Sync + 'static {
    fn display_text(&self) -> String;
}

/// Plain strings are usable directly (e.g. for build target selection).
impl SelectItem for String {
    fn display_text(&self) -> String {
        self.clone()
    }
}

// ── Concrete item types ───────────────────────────────────────────────────────

/// A build artifact application path with its associated tags.
pub struct AppItem {
    pub path: String,
    tag_names: Vec<String>,
    column_width: usize,
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
}

/// A build tag with its ELF modification time.
pub struct TagItem {
    pub name: String,
    pub modified: SystemTime,
    column_width: usize,
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
}

/// Creates a `Vec<TagItem>` from raw `(name, mtime)` pairs, computing the column
/// width over the provided set so timestamps align correctly across all items.
///
/// Accepts any slice, so callers can pre-filter (e.g. exclude the baseline tag)
/// and still get correct alignment for the remaining items.
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

// ── BuildArtifacts ────────────────────────────────────────────────────────────

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

// ── Generic selector ──────────────────────────────────────────────────────────

/// Presents an interactive skim fuzzy-find prompt and returns the selected item.
///
/// If `default_index` is `Some(i)`, item `i` is placed at the top of the list
/// so pressing Enter immediately accepts it. The returned value is the original
/// `T` — no string parsing is performed.
pub fn select<T: SelectItem>(
    prompt: &str,
    items: Vec<T>,
    default_index: Option<usize>,
) -> Result<T> {
    if items.is_empty() {
        return Err(eyre!("No items to select from."));
    }

    // Build display order: default item first, rest unchanged.
    let mut order: Vec<usize> = (0..items.len()).collect();
    if let Some(di) = default_index.filter(|&i| i < items.len()) {
        order.retain(|&i| i != di);
        order.insert(0, di);
    }

    let display_texts: Vec<String> = order.iter().map(|&i| items[i].display_text()).collect();
    let selected_text = fuzzy_select(prompt, display_texts)?;

    // Recover the original item by matching its display text exactly.
    // Display texts are deterministic and unique for our data (paths, tag+timestamp pairs).
    items
        .into_iter()
        .find(|item| item.display_text() == selected_text)
        .ok_or_else(|| eyre!("Selected item not found in original list"))
}

/// Core skim invocation: takes display strings, returns the one the user selected.
fn fuzzy_select(prompt: &str, items: Vec<String>) -> Result<String> {
    if items.is_empty() {
        return Err(eyre!("No items to select from."));
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
                out.selected_items
                    .into_iter()
                    .next()
                    .map(|i| i.output().to_string())
                    .ok_or_else(|| eyre!("No selection made."))
            }
        }
        Err(e) => {
            debug!("Skim returned error: {} - treated as cancellation", e);
            Err(eyre!("Selection process failed or was cancelled."))
        }
    }
}

// ── Misc helpers ──────────────────────────────────────────────────────────────

/// Constructs the relative path to an artifact: `"out/branch-builds/<tag>/<app_path>"`.
pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("out/branch-builds/{}/{}", tag, app_path)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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
    fn test_tag_item_display_alignment() {
        let entries = vec![
            ("short".to_string(), SystemTime::UNIX_EPOCH),
            ("a-longer-tag-name".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let items = create_tag_items(&entries);
        // Both items should have the same display width up to the "  (" separator
        let w0 = items[0].display_text().find("  (").unwrap();
        let w1 = items[1].display_text().find("  (").unwrap();
        assert_eq!(w0, w1);
    }

    #[test]
    fn test_select_item_for_string() {
        assert_eq!("hello".to_string().display_text(), "hello");
    }
}
