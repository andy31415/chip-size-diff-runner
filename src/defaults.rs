use eyre::{Result, WrapErr, eyre};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_RECENT_APPLICATIONS: usize = 10;

/// Persistent per-user settings stored in `~/.cache/branch_diff/defaults.toml`.
///
/// All fields are optional so the file can be absent or partially written
/// without breaking anything — missing fields deserialize to `None`/empty.
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ComparisonDefaults {
    pub workdir: Option<String>,
    /// Last-used baseline artifact path, relative to workdir.
    pub from_file: Option<String>,
    /// Last-used comparison artifact path, relative to workdir.
    pub to_file: Option<String>,
    /// Most-recently-used build targets, newest first. Capped at `MAX_RECENT_APPLICATIONS`.
    #[serde(default)]
    pub recent_applications: Vec<String>,
}

impl ComparisonDefaults {
    /// Loads settings from the cache file, returning `Default` if the file is absent or unparseable.
    ///
    /// Parse failures are silently ignored so a corrupt/outdated cache never blocks the tool.
    pub fn load() -> Result<Self> {
        let path = Self::cache_path()?;
        if !path.exists() {
            debug!("Defaults file not found: {}", path.display());
            return Ok(Self::default());
        }

        let mut file = fs::File::open(&path)
            .wrap_err_with(|| format!("Failed to open defaults file: {}", path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .wrap_err_with(|| format!("Failed to read defaults file: {}", path.display()))?;

        match toml::from_str(&contents) {
            Ok(defaults) => {
                debug!("Loaded defaults from {}: {:?}", path.display(), defaults);
                Ok(defaults)
            }
            Err(e) => {
                debug!(
                    "Ignoring unparseable defaults file {} ({}), using defaults",
                    path.display(),
                    e
                );
                Ok(Self::default())
            }
        }
    }

    /// Persists the current settings to the cache file, creating it if necessary.
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path()?;
        let toml_string =
            toml::to_string_pretty(self).wrap_err("Failed to serialize defaults to TOML")?;

        let mut file = fs::File::create(&path)
            .wrap_err_with(|| format!("Failed to create defaults file: {}", path.display()))?;
        file.write_all(toml_string.as_bytes())
            .wrap_err_with(|| format!("Failed to write defaults to file: {}", path.display()))?;
        debug!("Saved defaults to {}: {:?}", path.display(), self);
        Ok(())
    }

    /// Prepends `app` to `recent_applications` so the most-recently-used target
    /// appears first in the build selector. Deduplicates and caps the list.
    pub fn add_recent_application(&mut self, app: &str) {
        self.recent_applications.retain(|a| a != app);
        self.recent_applications.insert(0, app.to_string());
        self.recent_applications.truncate(MAX_RECENT_APPLICATIONS);
    }

    /// Returns the path to the cache file, creating the cache directory if needed.
    fn cache_path() -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| eyre!("Could not find cache directory"))?
            .join("branch_diff");
        fs::create_dir_all(&cache_dir).wrap_err("Failed to create cache directory")?;
        Ok(cache_dir.join("defaults.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_recent_application_prepends() {
        let mut d = ComparisonDefaults::default();
        d.add_recent_application("app-a");
        d.add_recent_application("app-b");
        assert_eq!(d.recent_applications, vec!["app-b", "app-a"]);
    }

    #[test]
    fn test_add_recent_application_deduplicates() {
        let mut d = ComparisonDefaults::default();
        d.add_recent_application("app-a");
        d.add_recent_application("app-b");
        d.add_recent_application("app-a"); // moves to front, no duplicate
        assert_eq!(d.recent_applications, vec!["app-a", "app-b"]);
    }

    #[test]
    fn test_add_recent_application_caps_at_max() {
        let mut d = ComparisonDefaults::default();
        for i in 0..=(MAX_RECENT_APPLICATIONS + 2) {
            d.add_recent_application(&format!("app-{}", i));
        }
        assert_eq!(d.recent_applications.len(), MAX_RECENT_APPLICATIONS);
    }

    #[test]
    fn test_round_trip_serialization() {
        let mut original = ComparisonDefaults::default();
        original.workdir = Some("/some/path".to_string());
        original.add_recent_application("linux-x64-all-clusters-app");
        original.add_recent_application("efr32-brd4187c-lock-no-version");

        let toml_str = toml::to_string_pretty(&original).unwrap();
        let restored: ComparisonDefaults = toml::from_str(&toml_str).unwrap();

        assert_eq!(restored.workdir, original.workdir);
        assert_eq!(restored.recent_applications, original.recent_applications);
    }
}
