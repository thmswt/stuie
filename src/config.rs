use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub name: String,
    pub provider_type: ProviderType,
    pub credentials_path: Option<String>,
    pub region: Option<String>,
    pub bucket_name: Option<String>,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub allow_http: Option<bool>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            provider_type: ProviderType::default(),
            credentials_path: None,
            region: None,
            bucket_name: None,
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            allow_http: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum ProviderType {
    #[default]
    S3,
    GCS,
    Azure,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return Ok(Config::default());
        }
        let content = fs::read_to_string(config_path).context("Failed to read config file")?;
        let config: Config = serde_json::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        let content = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(config_path, content).context("Failed to write config file")?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".config").join("stuie").join("config.json"))
    }
}
