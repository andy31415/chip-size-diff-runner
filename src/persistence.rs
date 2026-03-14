use eyre::{Result, WrapErr, eyre};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_RECENT_APPLICATIONS: usize = 10;

/// Persistent per-user session state stored in `~/.cache/branch_diff/session.toml`.
///
/// All fields are optional so the file can be absent or partially written
/// without breaking anything — missing fields deserialize to `None`/empty.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionState {
    pub workdir: Option<String>,
    /// Last-used baseline artifact path, relative to workdir.
    pub from_file: Option<String>,
    /// Last-used comparison artifact path, relative to workdir.
    pub to_file: Option<String>,
    /// Most-recently-used build targets, newest first. Capped at `MAX_RECENT_APPLICATIONS`.
    #[serde(default)]
    pub recent_applications: Vec<String>,
    /// Common targets shown as fallbacks.
    pub default_targets: Vec<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            workdir: None,
            from_file: None,
            to_file: None,
            recent_applications: Vec::new(),
            default_targets: vec![
                "linux-x64-all-clusters-app".to_string(),
                "linux-x64-chip-tool".to_string(),
                "linux-x64-all-devices".to_string(),
                "efr32-brd4187c-lock-no-version".to_string(),
                "stm32-stm32wb5mm-dk-light".to_string(),
                "qpg-qpg6200-light".to_string(),
                "ti-cc13x4_26x4-lock-ftd".to_string(),
            ],
        }
    }
}

impl SessionState {
    /// Loads session state from the cache file, returning `Default` if the file is absent or unparseable.
    ///
    /// Parse failures are silently ignored so a corrupt/outdated cache never blocks the tool.
    pub fn load() -> Result<Self> {
        let path = Self::cache_path()?;
        if !path.exists() {
            debug!("Session file not found: {}", path.display());
            return Ok(Self::default());
        }

        let mut file = fs::File::open(&path)
            .wrap_err_with(|| format!("Failed to open session file: {}", path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .wrap_err_with(|| format!("Failed to read session file: {}", path.display()))?;

        match toml::from_str(&contents) {
            Ok(state) => {
                debug!("Loaded session state from {}: {:?}", path.display(), state);
                Ok(state)
            }
            Err(e) => {
                debug!(
                    "Ignoring unparseable session file {} ({}), using defaults",
                    path.display(),
                    e
                );
                Ok(Self::default())
            }
        }
    }

    /// Persists the current session state to the cache file, creating it if necessary.
    pub fn save(&self) -> Result<()> {
        let path = Self::cache_path()?;
        let toml_string =
            toml::to_string_pretty(self).wrap_err("Failed to serialize session state to TOML")?;

        let mut file = fs::File::create(&path)
            .wrap_err_with(|| format!("Failed to create session file: {}", path.display()))?;
        file.write_all(toml_string.as_bytes()).wrap_err_with(|| {
            format!("Failed to write session state to file: {}", path.display())
        })?;
        debug!("Saved session state to {}: {:?}", path.display(), self);
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
        Ok(cache_dir.join("session.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_recent_application_prepends() {
        let mut s = SessionState::default();
        s.add_recent_application("app-a");
        s.add_recent_application("app-b");
        assert_eq!(s.recent_applications, vec!["app-b", "app-a"]);
    }

    #[test]
    fn test_add_recent_application_deduplicates() {
        let mut s = SessionState::default();
        s.add_recent_application("app-a");
        s.add_recent_application("app-b");
        s.add_recent_application("app-a");
        assert_eq!(s.recent_applications, vec!["app-a", "app-b"]);
    }

    #[test]
    fn test_add_recent_application_caps_at_max() {
        let mut s = SessionState::default();
        for i in 0..=(MAX_RECENT_APPLICATIONS + 2) {
            s.add_recent_application(&format!("app-{}", i));
        }
        assert_eq!(s.recent_applications.len(), MAX_RECENT_APPLICATIONS);
    }
}
