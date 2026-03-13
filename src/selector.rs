use chrono::{DateTime, Local};
use eyre::{Result, WrapErr, eyre};
use goblin::elf::Elf;
use log::debug;
use owo_colors::OwoColorize;
use skim::prelude::{Skim, SkimItemReader, SkimItemReaderOption, SkimOptionsBuilder};
use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

// ── SelectItem trait ─────────────────────────────────────────────────────────

/// Items that can be presented in a skim fuzzy-find prompt.
///
/// `display_text()` is the plain text used for fuzzy matching and item
/// recovery after selection. Override `skim_text()` to add ANSI decoration
/// (colours, dimming) that skim will render — the decoration is stripped
/// before recovery so `display_text()` stays clean.
pub trait SelectItem: Send + Sync + 'static {
    fn display_text(&self) -> String;

    /// ANSI-decorated text shown in the skim UI. Defaults to `display_text()`.
    fn skim_text(&self) -> String {
        self.display_text()
    }
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

    fn skim_text(&self) -> String {
        format!(
            "{:<width$} {}",
            self.path,
            format!("(Tags: {})", self.tag_names.join(", ")).dimmed(),
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

    fn skim_text(&self) -> String {
        let dt: DateTime<Local> = self.modified.into();
        format!(
            "{:<width$}  {}",
            self.name,
            format!("({})", dt.format("%Y-%m-%d %H:%M:%S")).dimmed(),
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

    // Pass skim_text() for display (may include ANSI). After selection, strip
    // ANSI from skim's output and match against plain display_text() for recovery.
    let skim_texts: Vec<String> = order.iter().map(|&i| items[i].skim_text()).collect();
    let selected_raw = fuzzy_select(prompt, skim_texts)?;
    let selected_plain = strip_ansi_codes(&selected_raw);

    items
        .into_iter()
        .find(|item| item.display_text() == selected_plain)
        .ok_or_else(|| eyre!("Selected item not found in original list"))
}

/// Core skim invocation: takes ANSI-decorated display strings, returns the raw
/// output of the item the user selected (may include ANSI codes).
fn fuzzy_select(prompt: &str, items: Vec<String>) -> Result<String> {
    if items.is_empty() {
        return Err(eyre!("No items to select from."));
    }

    let options = SkimOptionsBuilder::default()
        .prompt(format!("{}: ", prompt))
        .ansi(true)
        .build()
        .wrap_err("Failed to build Skim options")?;

    let item_string = items.join("\n");
    // Must enable ANSI on the reader too — SkimItemReader::default() has ANSI
    // disabled, causing escape codes to be mangled before output() is called.
    let item_reader = SkimItemReader::new(SkimItemReaderOption::default().ansi(true));
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

/// Strips CSI escape sequences (`\x1b[...m`) from a string, returning plain text.
fn strip_ansi_codes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
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
    fn test_tag_item_skim_text_alignment() {
        let entries = vec![
            ("short".to_string(), SystemTime::UNIX_EPOCH),
            ("a-longer-tag-name".to_string(), SystemTime::UNIX_EPOCH),
        ];
        let items = create_tag_items(&entries);
        // skim_text aligns the tag column; the ANSI dim code appears at the same offset
        let w0 = items[0].skim_text().find("  \x1b[2m(").unwrap();
        let w1 = items[1].skim_text().find("  \x1b[2m(").unwrap();
        assert_eq!(w0, w1);
    }

    #[test]
    fn test_select_item_for_string() {
        assert_eq!("hello".to_string().display_text(), "hello");
    }

    #[test]
    fn test_create_tag_items_empty() {
        let items = create_tag_items(&[]);
        assert!(items.is_empty());
    }

    #[test]
    fn test_app_items_alignment() {
        use std::time::SystemTime;
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
        // display_text aligns the path column; "(Tags:" appears at the same offset
        let positions: Vec<usize> = items
            .iter()
            .map(|i| i.display_text().find(" (Tags:").unwrap())
            .collect();
        assert!(positions.iter().all(|&p| p == positions[0]));
    }

    #[test]
    fn test_strip_ansi_codes() {
        assert_eq!(strip_ansi_codes("hello"), "hello");
        assert_eq!(strip_ansi_codes("\x1b[2mhello\x1b[0m"), "hello");
        assert_eq!(
            strip_ansi_codes("tag  \x1b[2m(2024-01-15)\x1b[0m"),
            "tag  (2024-01-15)"
        );
    }

    #[test]
    fn test_display_text_matches_stripped_skim_text() {
        let entries = vec![("my-tag".to_string(), SystemTime::UNIX_EPOCH)];
        let items = create_tag_items(&entries);
        let item = &items[0];
        assert_eq!(item.display_text(), strip_ansi_codes(&item.skim_text()));
    }
}
