use dialoguer::{Select, theme::ColorfulTheme};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct BuildArtifacts {
    // App Path -> Sorted list of Tags
    pub apps: BTreeMap<String, Vec<String>>,
}

impl BuildArtifacts {
    pub fn find(workdir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let builds_dir = workdir.join("out/branch-builds");
        let mut apps: BTreeMap<String, Vec<String>> = BTreeMap::new();

        if !builds_dir.exists() {
            return Ok(BuildArtifacts { apps });
        }

        for entry in WalkDir::new(&builds_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file()
                && let Some(filename) = entry.path().file_name().and_then(|n| n.to_str())
                    && (!filename.contains('.') || filename.ends_with(".elf") || filename.ends_with(".bin")) {
                        let relative_path = entry.path().strip_prefix(&builds_dir)?;
                        let components: Vec<&str> = relative_path.iter().map(|s| s.to_str().unwrap_or("")).collect();

                        if components.len() > 1 {
                            let tag = components[0].to_string();
                            let app_path = PathBuf::from_iter(&components[1..]).to_string_lossy().to_string();
                            apps.entry(app_path).or_default().push(tag);
                        }
                    }
        }

        // Sort tags for each app
        for tags in apps.values_mut() {
            tags.sort();
            tags.dedup();
        }

        Ok(BuildArtifacts { apps })
    }

    pub fn get_app_paths(&self) -> Vec<&String> {
        self.apps.keys().collect()
    }

    pub fn get_tags_for_app(&self, app_path: &str) -> Option<&Vec<String>> {
        self.apps.get(app_path)
    }
}

pub fn select_string(prompt: &str, items: &[&String]) -> Result<String, Box<dyn std::error::Error>> {
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

pub fn build_path(tag: &str, app_path: &str) -> String {
    format!("out/branch-builds/{}/{}", tag, app_path)
}
