use eyre::{Result, WrapErr, eyre};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ComparisonDefaults {
    pub workdir: Option<String>,
    pub from_file: Option<String>,
    pub to_file: Option<String>,
}

fn get_cache_file_path() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| eyre!("Could not find cache directory"))?
        .join("branch_diff");
    fs::create_dir_all(&cache_dir).wrap_err("Failed to create cache directory")?;
    Ok(cache_dir.join("defaults.toml"))
}

/// Loads the comparison defaults from the cache file.
pub fn load_defaults() -> Result<ComparisonDefaults> {
    let path = get_cache_file_path().wrap_err("Failed to get cache file path")?;
    if !path.exists() {
        debug!("Defaults file not found: {}", path.display());
        return Ok(ComparisonDefaults::default());
    }

    let mut file = fs::File::open(&path)
        .wrap_err_with(|| format!("Failed to open defaults file: {}", path.display()))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .wrap_err_with(|| format!("Failed to read defaults file: {}", path.display()))?;

    match toml::from_str(&contents) {
        Ok(defaults) => {
            debug!("Loaded defaults: {:?} from {}", defaults, path.display());
            Ok(defaults)
        }
        Err(e) => {
            debug!(
                "Failed to parse defaults file {}: {}, using default",
                path.display(),
                e
            );
            Ok(ComparisonDefaults::default())
        }
    }
}

/// Saves the given comparison defaults to the cache file.
pub fn save_defaults(defaults: &ComparisonDefaults) -> Result<()> {
    let path = get_cache_file_path().wrap_err("Failed to get cache file path")?;
    let toml_string =
        toml::to_string_pretty(defaults).wrap_err("Failed to serialize defaults to TOML")?;

    let mut file = fs::File::create(&path)
        .wrap_err_with(|| format!("Failed to create defaults file: {}", path.display()))?;
    file.write_all(toml_string.as_bytes())
        .wrap_err_with(|| format!("Failed to write defaults to file: {}", path.display()))?;
    debug!("Saved defaults: {:?} to {}", defaults, path.display());
    Ok(())
}
