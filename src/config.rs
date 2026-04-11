use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub keymap: KeymapConfig,
    #[serde(default)]
    pub settings: SettingsConfig,
    // Legacy field — migrated to settings.show_icons on load
    #[serde(default = "default_show_icons", skip_serializing)]
    pub show_icons: bool,
}

fn default_show_icons() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeymapConfig {
    // Normal Mode — Navigation
    #[serde(default = "default_key_move_down")]
    pub move_down: String,
    #[serde(default = "default_key_move_up")]
    pub move_up: String,
    #[serde(default = "default_key_enter")]
    pub enter: String,
    #[serde(default = "default_key_back")]
    pub back: String,
    #[serde(default = "default_key_switch_pane")]
    pub switch_pane: String,
    // Normal Mode — Actions
    #[serde(default = "default_key_quit")]
    pub quit: String,
    #[serde(default = "default_key_help")]
    pub help: String,
    #[serde(default = "default_key_filter")]
    pub filter: String,
    #[serde(default = "default_key_select")]
    pub select: String,
    #[serde(default = "default_key_copy_path")]
    pub copy_path: String,
    #[serde(default = "default_key_new_folder")]
    pub new_folder: String,
    #[serde(default = "default_key_download")]
    pub download: String,
    #[serde(default = "default_key_upload")]
    pub upload: String,
    #[serde(default = "default_key_delete")]
    pub delete: String,
    #[serde(default = "default_key_config")]
    pub config: String,
    #[serde(default = "default_key_add_provider")]
    pub add_provider: String,
    // Edit Mode
    #[serde(default = "default_key_edit_next_field")]
    pub edit_next_field: String,
    #[serde(default = "default_key_edit_prev_field")]
    pub edit_prev_field: String,
    #[serde(default = "default_key_edit_confirm")]
    pub edit_confirm: String,
    #[serde(default = "default_key_edit_cancel")]
    pub edit_cancel: String,
    #[serde(default = "default_key_edit_cycle_down")]
    pub edit_cycle_down: String,
    #[serde(default = "default_key_edit_cycle_up")]
    pub edit_cycle_up: String,
}

impl Default for KeymapConfig {
    fn default() -> Self {
        Self {
            move_down: default_key_move_down(),
            move_up: default_key_move_up(),
            enter: default_key_enter(),
            back: default_key_back(),
            switch_pane: default_key_switch_pane(),
            quit: default_key_quit(),
            help: default_key_help(),
            filter: default_key_filter(),
            select: default_key_select(),
            copy_path: default_key_copy_path(),
            new_folder: default_key_new_folder(),
            download: default_key_download(),
            upload: default_key_upload(),
            delete: default_key_delete(),
            config: default_key_config(),
            add_provider: default_key_add_provider(),
            edit_next_field: default_key_edit_next_field(),
            edit_prev_field: default_key_edit_prev_field(),
            edit_confirm: default_key_edit_confirm(),
            edit_cancel: default_key_edit_cancel(),
            edit_cycle_down: default_key_edit_cycle_down(),
            edit_cycle_up: default_key_edit_cycle_up(),
        }
    }
}

// Normal Mode — Navigation
fn default_key_move_down() -> String { "j".to_string() }
fn default_key_move_up() -> String { "k".to_string() }
fn default_key_enter() -> String { "Enter".to_string() }
fn default_key_back() -> String { "Backspace".to_string() }
fn default_key_switch_pane() -> String { "Tab".to_string() }
// Normal Mode — Actions
fn default_key_quit() -> String { "q".to_string() }
fn default_key_help() -> String { "?".to_string() }
fn default_key_filter() -> String { "/".to_string() }
fn default_key_select() -> String { "Space".to_string() }
fn default_key_copy_path() -> String { "y".to_string() }
fn default_key_new_folder() -> String { "n".to_string() }
fn default_key_download() -> String { "d".to_string() }
fn default_key_upload() -> String { "u".to_string() }
fn default_key_delete() -> String { "x".to_string() }
fn default_key_config() -> String { "c".to_string() }
fn default_key_add_provider() -> String { "a".to_string() }
// Edit Mode
fn default_key_edit_next_field() -> String { "Tab".to_string() }
fn default_key_edit_prev_field() -> String { "S-Tab".to_string() }
fn default_key_edit_confirm() -> String { "Enter".to_string() }
fn default_key_edit_cancel() -> String { "Esc".to_string() }
fn default_key_edit_cycle_down() -> String { "j".to_string() }
fn default_key_edit_cycle_up() -> String { "k".to_string() }

impl KeymapConfig {
    /// Total number of editable fields across all sections.
    pub const FIELD_COUNT: usize = 22;

    /// Returns a section header to display *before* this index, if any.
    pub fn section_header(index: usize) -> Option<&'static str> {
        match index {
            0 => Some("Normal Mode"),
            16 => Some("Edit Mode"),
            _ => None,
        }
    }

    pub fn field_name(index: usize) -> &'static str {
        match index {
            // Normal Mode — Navigation
            0 => "Move Down",
            1 => "Move Up",
            2 => "Enter / Open",
            3 => "Back",
            4 => "Switch Pane",
            // Normal Mode — Actions
            5 => "Quit",
            6 => "Help",
            7 => "Filter",
            8 => "Select",
            9 => "Copy Path",
            10 => "New Folder",
            11 => "Download",
            12 => "Upload",
            13 => "Delete",
            14 => "Config",
            15 => "Add Provider",
            // Edit Mode
            16 => "Next Field",
            17 => "Prev Field",
            18 => "Confirm",
            19 => "Cancel",
            20 => "Cycle Down",
            21 => "Cycle Up",
            _ => "",
        }
    }

    pub fn get_field(&self, index: usize) -> &str {
        match index {
            0 => &self.move_down,
            1 => &self.move_up,
            2 => &self.enter,
            3 => &self.back,
            4 => &self.switch_pane,
            5 => &self.quit,
            6 => &self.help,
            7 => &self.filter,
            8 => &self.select,
            9 => &self.copy_path,
            10 => &self.new_folder,
            11 => &self.download,
            12 => &self.upload,
            13 => &self.delete,
            14 => &self.config,
            15 => &self.add_provider,
            16 => &self.edit_next_field,
            17 => &self.edit_prev_field,
            18 => &self.edit_confirm,
            19 => &self.edit_cancel,
            20 => &self.edit_cycle_down,
            21 => &self.edit_cycle_up,
            _ => "",
        }
    }

    pub fn set_field(&mut self, index: usize, value: String) {
        match index {
            0 => self.move_down = value,
            1 => self.move_up = value,
            2 => self.enter = value,
            3 => self.back = value,
            4 => self.switch_pane = value,
            5 => self.quit = value,
            6 => self.help = value,
            7 => self.filter = value,
            8 => self.select = value,
            9 => self.copy_path = value,
            10 => self.new_folder = value,
            11 => self.download = value,
            12 => self.upload = value,
            13 => self.delete = value,
            14 => self.config = value,
            15 => self.add_provider = value,
            16 => self.edit_next_field = value,
            17 => self.edit_prev_field = value,
            18 => self.edit_confirm = value,
            19 => self.edit_cancel = value,
            20 => self.edit_cycle_down = value,
            21 => self.edit_cycle_up = value,
            _ => {}
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettingsConfig {
    #[serde(default = "default_show_icons")]
    pub show_icons: bool,
    #[serde(default = "default_confirm_delete")]
    pub confirm_delete: bool,
    #[serde(default = "default_preview_max_bytes")]
    pub preview_max_bytes: usize,
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            show_icons: default_show_icons(),
            confirm_delete: default_confirm_delete(),
            preview_max_bytes: default_preview_max_bytes(),
        }
    }
}

fn default_confirm_delete() -> bool { true }
fn default_preview_max_bytes() -> usize { 1024 }

impl SettingsConfig {
    pub fn field_name(index: usize) -> &'static str {
        match index {
            0 => "Show Icons",
            1 => "Confirm Delete",
            2 => "Preview Max Bytes",
            _ => "",
        }
    }

    pub const FIELD_COUNT: usize = 3;
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
        let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
        let mut config: Config = serde_json::from_str(&content).context("Failed to parse config file")?;
        // Migrate legacy top-level show_icons into settings
        if content.contains("\"show_icons\"") && !content.contains("\"settings\"") {
            config.settings.show_icons = config.show_icons;
        }
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
