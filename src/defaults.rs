use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ComparisonDefaults {
    pub from_file: Option<String>,
    pub to_file: Option<String>,
}

fn get_cache_file_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cache_dir = dirs::cache_dir()
        .ok_or("Could not find cache directory")?
        .join("branch_diff");
    fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir.join("defaults.toml"))
}

/// Loads the comparison defaults from the cache file.
pub fn load_defaults() -> Result<ComparisonDefaults, Box<dyn std::error::Error>> {
    let path = get_cache_file_path()?;
    if !path.exists() {
        debug!("Defaults file not found: {}", path.display());
        return Ok(ComparisonDefaults::default());
    }

    let mut file = fs::File::open(&path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

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
pub fn save_defaults(defaults: &ComparisonDefaults) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_cache_file_path()?;
    let toml_string = toml::to_string_pretty(defaults)?;

    let mut file = fs::File::create(&path)?;
    file.write_all(toml_string.as_bytes())?;
    debug!("Saved defaults: {:?} to {}", defaults, path.display());
    Ok(())
}
