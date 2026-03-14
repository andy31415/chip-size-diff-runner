use eyre::{Result, WrapErr, eyre};
use log::debug;
use skim::prelude::{Skim, SkimItemReader, SkimItemReaderOption, SkimOptionsBuilder};
use std::io::Cursor;

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

    let order = build_ordered_indices(items.len(), default_index);

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

/// Returns item indices in display order: if `default_index` is valid, it is
/// placed first; all others follow in their original order.
fn build_ordered_indices(len: usize, default_index: Option<usize>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if let Some(di) = default_index.filter(|&i| i < len) {
        order.retain(|&i| i != di);
        order.insert(0, di);
    }
    order
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
pub fn strip_ansi_codes(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_item_for_string() {
        assert_eq!("hello".to_string().display_text(), "hello");
    }

    #[test]
    fn test_build_ordered_indices_no_default_is_identity() {
        assert_eq!(build_ordered_indices(3, None), vec![0, 1, 2]);
    }

    #[test]
    fn test_build_ordered_indices_default_moved_to_front() {
        assert_eq!(build_ordered_indices(4, Some(2)), vec![2, 0, 1, 3]);
    }

    #[test]
    fn test_build_ordered_indices_default_already_first() {
        assert_eq!(build_ordered_indices(3, Some(0)), vec![0, 1, 2]);
    }

    #[test]
    fn test_build_ordered_indices_out_of_bounds_ignored() {
        assert_eq!(build_ordered_indices(3, Some(5)), vec![0, 1, 2]);
    }

    #[test]
    fn test_build_ordered_indices_single_item() {
        assert_eq!(build_ordered_indices(1, Some(0)), vec![0]);
        assert_eq!(build_ordered_indices(1, None), vec![0]);
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
}
