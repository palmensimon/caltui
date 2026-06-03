use anyhow::Result;
use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleConfig {
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub refresh_token: String,
    // Fallback options if OAuth is not configured
    #[serde(default)]
    pub vdir_path: String,
    #[serde(default)]
    pub ics_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MicrosoftConfig {
    #[serde(default)]
    pub ics_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub start_hour: u32,
    pub end_hour: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { start_hour: 8, end_hour: 18 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub google: GoogleConfig,
    #[serde(default)]
    pub microsoft: MicrosoftConfig,
    #[serde(default)]
    pub display: DisplayConfig,
}

fn config_path() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("caltui")
        .join("config.toml")
}

pub fn load_config() -> Config {
    let path = config_path();
    if !path.exists() {
        return Config::default();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };
    toml::from_str(&content).unwrap_or_default()
}

pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}
