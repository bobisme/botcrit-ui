//! User configuration handling

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme: Option<String>,
}

pub fn load_ui_config() -> anyhow::Result<Option<UiConfig>> {
    let Some(path) = config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let config = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse config: {}", path.display()))?;
    Ok(Some(config))
}

pub fn save_ui_config(config: &UiConfig) -> anyhow::Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, contents)?;
    Ok(())
}

fn config_path() -> Option<PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        Path::new(&home).join(".config")
    } else {
        return None;
    };

    Some(base.join(".botcrit").join("ui.json"))
}
