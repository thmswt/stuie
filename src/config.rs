use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub keymap: KeymapConfig,
    #[serde(default)]
    pub settings: SettingsConfig,
    #[serde(default)]
    pub theme: String,
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
    #[serde(default = "default_key_toggle_preview")]
    pub toggle_preview: String,
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
            toggle_preview: default_key_toggle_preview(),
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
fn default_key_move_down() -> String {
    "j".to_string()
}
fn default_key_move_up() -> String {
    "k".to_string()
}
fn default_key_enter() -> String {
    "Enter".to_string()
}
fn default_key_back() -> String {
    "Backspace".to_string()
}
fn default_key_switch_pane() -> String {
    "Tab".to_string()
}
// Normal Mode — Actions
fn default_key_quit() -> String {
    "q".to_string()
}
fn default_key_help() -> String {
    "?".to_string()
}
fn default_key_filter() -> String {
    "/".to_string()
}
fn default_key_select() -> String {
    "Space".to_string()
}
fn default_key_copy_path() -> String {
    "y".to_string()
}
fn default_key_new_folder() -> String {
    "n".to_string()
}
fn default_key_download() -> String {
    "d".to_string()
}
fn default_key_upload() -> String {
    "u".to_string()
}
fn default_key_delete() -> String {
    "x".to_string()
}
fn default_key_config() -> String {
    "c".to_string()
}
fn default_key_add_provider() -> String {
    "a".to_string()
}
fn default_key_toggle_preview() -> String {
    "p".to_string()
}
// Edit Mode
fn default_key_edit_next_field() -> String {
    "Tab".to_string()
}
fn default_key_edit_prev_field() -> String {
    "S-Tab".to_string()
}
fn default_key_edit_confirm() -> String {
    "Enter".to_string()
}
fn default_key_edit_cancel() -> String {
    "Esc".to_string()
}
fn default_key_edit_cycle_down() -> String {
    "j".to_string()
}
fn default_key_edit_cycle_up() -> String {
    "k".to_string()
}

impl KeymapConfig {
    /// Total number of editable fields across all sections.
    pub const FIELD_COUNT: usize = 23;

    /// Returns a section header to display *before* this index, if any.
    pub fn section_header(index: usize) -> Option<&'static str> {
        match index {
            0 => Some("Normal Mode"),
            17 => Some("Edit Mode"),
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
            16 => "Toggle Preview",
            // Edit Mode
            17 => "Next Field",
            18 => "Prev Field",
            19 => "Confirm",
            20 => "Cancel",
            21 => "Cycle Down",
            22 => "Cycle Up",
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
            16 => &self.toggle_preview,
            17 => &self.edit_next_field,
            18 => &self.edit_prev_field,
            19 => &self.edit_confirm,
            20 => &self.edit_cancel,
            21 => &self.edit_cycle_down,
            22 => &self.edit_cycle_up,
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
            16 => self.toggle_preview = value,
            17 => self.edit_next_field = value,
            18 => self.edit_prev_field = value,
            19 => self.edit_confirm = value,
            20 => self.edit_cancel = value,
            21 => self.edit_cycle_down = value,
            22 => self.edit_cycle_up = value,
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
    #[serde(default = "default_log_to_file")]
    pub log_to_file: bool,
    #[serde(default = "default_log_file_path")]
    pub log_file_path: String,
    #[serde(default = "default_diagnostics_enabled")]
    pub diagnostics_enabled: bool,
    #[serde(default = "default_diagnostics_log_dir")]
    pub diagnostics_log_dir: String,
}

impl Default for SettingsConfig {
    fn default() -> Self {
        Self {
            show_icons: default_show_icons(),
            confirm_delete: default_confirm_delete(),
            preview_max_bytes: default_preview_max_bytes(),
            log_to_file: default_log_to_file(),
            log_file_path: default_log_file_path(),
            diagnostics_enabled: default_diagnostics_enabled(),
            diagnostics_log_dir: default_diagnostics_log_dir(),
        }
    }
}

fn default_confirm_delete() -> bool {
    true
}
fn default_preview_max_bytes() -> usize {
    1024
}
fn default_log_to_file() -> bool {
    false
}
fn default_log_file_path() -> String {
    "~/.stuie/session.log".to_string()
}
fn default_diagnostics_enabled() -> bool {
    true
}
fn default_diagnostics_log_dir() -> String {
    "logs".to_string()
}

impl SettingsConfig {
    pub fn field_name(index: usize) -> &'static str {
        match index {
            0 => "Show Icons",
            1 => "Confirm Delete",
            2 => "Preview Max Bytes",
            3 => "Log Tasks To File",
            4 => "Log File Path",
            5 => "Diagnostics Enabled",
            6 => "Diagnostics Log Dir",
            _ => "",
        }
    }

    pub const FIELD_COUNT: usize = 7;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum GcsAuthMode {
    /// Service-account JSON key referenced by `credentials_path`, or ADC
    /// detected automatically. Single-project behaviour (legacy).
    ServiceAccount,
    /// Use the gcloud CLI's Application Default Credentials directly.
    /// No secrets are stored in the config; only the selected project IDs.
    GcloudAdc,
}

impl Default for GcsAuthMode {
    fn default() -> Self {
        GcsAuthMode::ServiceAccount
    }
}

fn default_gcs_auth_mode() -> GcsAuthMode {
    GcsAuthMode::ServiceAccount
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
    pub project: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// GCS-only: which authentication mechanism to use. For non-GCS providers
    /// this field is ignored. Defaults to `ServiceAccount` to keep legacy
    /// configs working unchanged.
    #[serde(default = "default_gcs_auth_mode")]
    pub gcs_auth_mode: GcsAuthMode,
    /// GCS-only (GcloudAdc mode): project IDs the user has explicitly
    /// selected. Buckets are listed across every project in this list.
    #[serde(default)]
    pub oauth_projects: Vec<String>,
    /// When true, this provider is wrapped in a `ReadOnlyStorage` decorator
    /// at startup that hard-blocks every mutating operation (upload,
    /// create_folder, delete_objects). The flag exists to give a single
    /// place to guarantee that no destructive call slips through, no matter
    /// what UI path a future bug might take.
    #[serde(default)]
    pub read_only: bool,
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
            project: None,
            enabled: true,
            gcs_auth_mode: GcsAuthMode::default(),
            oauth_projects: Vec::new(),
            read_only: false,
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

impl ProviderType {
    pub fn scheme(&self) -> &'static str {
        match self {
            ProviderType::S3 => "s3",
            ProviderType::GCS => "gs",
            ProviderType::Azure => "az",
        }
    }

    /// Legacy/minimum field count. For GCS this returns the ServiceAccount
    /// layout. For provider-aware UI code use `ProviderConfig::field_count`.
    pub fn field_count(&self) -> usize {
        match self {
            ProviderType::S3 | ProviderType::Azure => 9,
            ProviderType::GCS => 7,
        }
    }

    /// Legacy field names (ServiceAccount-mode layout for GCS). For
    /// provider-aware UI code use `ProviderConfig::field_names`.
    pub fn field_names(&self) -> &'static [&'static str] {
        match self {
            ProviderType::S3 | ProviderType::Azure => FIELDS_S3_AZURE,
            ProviderType::GCS => FIELDS_GCS_SERVICE_ACCOUNT,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider config field layouts
// ---------------------------------------------------------------------------
//
// GCS has two layouts driven by `gcs_auth_mode`. Both share indices 0..=2
// (Name, Type, Auth Mode) so switching modes in the UI doesn't jump the
// cursor unexpectedly.

const FIELDS_S3_AZURE: &[&str] = &[
    "Name *",
    "Type *",
    "Region *",
    "Bucket Name (Optional)",
    "Endpoint (Optional)",
    "Access Key *",
    "Secret Key *",
    "Allow HTTP (Optional)",
    "Read-Only Mode",
];

const FIELDS_GCS_SERVICE_ACCOUNT: &[&str] = &[
    "Name *",
    "Type *",
    "Auth Mode *",
    "Project",
    "Bucket Name (Optional)",
    "Credentials Path (Optional)",
    "Read-Only Mode",
];

const FIELDS_GCS_GCLOUD_ADC: &[&str] = &[
    "Name *",
    "Type *",
    "Auth Mode *",
    "ADC Status",
    "Selected Projects",
    "Read-Only Mode",
];

impl ProviderConfig {
    /// Field list for this provider, taking `gcs_auth_mode` into account.
    pub fn field_names(&self) -> &'static [&'static str] {
        match self.provider_type {
            ProviderType::S3 | ProviderType::Azure => FIELDS_S3_AZURE,
            ProviderType::GCS => match self.gcs_auth_mode {
                GcsAuthMode::ServiceAccount => FIELDS_GCS_SERVICE_ACCOUNT,
                GcsAuthMode::GcloudAdc => FIELDS_GCS_GCLOUD_ADC,
            },
        }
    }

    /// Number of fields visible in the editor for this provider.
    pub fn field_count(&self) -> usize {
        self.field_names().len()
    }

    /// Returns the display value for a config field by index.
    /// Provider-type and auth-mode aware.
    pub fn get_field_value(&self, index: usize) -> String {
        // The Read-Only Mode field always sits at the tail of whichever
        // layout this provider uses, so handle it uniformly here.
        if self.is_read_only_field(index) {
            return self.read_only.to_string();
        }
        match self.provider_type {
            ProviderType::S3 | ProviderType::Azure => match index {
                0 => self.name.clone(),
                1 => format!("{:?}", self.provider_type),
                2 => self.region.clone().unwrap_or_default(),
                3 => self.bucket_name.clone().unwrap_or_default(),
                4 => self.endpoint.clone().unwrap_or_default(),
                5 => self.access_key_id.clone().unwrap_or_default(),
                6 => self.secret_access_key.clone().unwrap_or_default(),
                7 => self.allow_http.unwrap_or(false).to_string(),
                _ => String::new(),
            },
            ProviderType::GCS => match (self.gcs_auth_mode.clone(), index) {
                (_, 0) => self.name.clone(),
                (_, 1) => format!("{:?}", self.provider_type),
                (GcsAuthMode::ServiceAccount, 2) => "Service Account".to_string(),
                (GcsAuthMode::GcloudAdc, 2) => "gcloud ADC".to_string(),
                (GcsAuthMode::ServiceAccount, 3) => self.project.clone().unwrap_or_default(),
                (GcsAuthMode::ServiceAccount, 4) => self.bucket_name.clone().unwrap_or_default(),
                (GcsAuthMode::ServiceAccount, 5) => {
                    self.credentials_path.clone().unwrap_or_default()
                }
                (GcsAuthMode::GcloudAdc, 3) => {
                    // Actual ADC presence/absence is resolved lazily by the
                    // UI layer (it reads the filesystem). Show a neutral
                    // placeholder here so this method stays pure.
                    "(run `gcloud auth application-default login`)".to_string()
                }
                (GcsAuthMode::GcloudAdc, 4) => {
                    if self.oauth_projects.is_empty() {
                        "(none — press P to select)".to_string()
                    } else {
                        format!(
                            "{} selected: {}",
                            self.oauth_projects.len(),
                            self.oauth_projects.join(", ")
                        )
                    }
                }
                _ => String::new(),
            },
        }
    }

    /// Index of the Read-Only Mode field for this layout (always the last
    /// field), or `None` if the index is out of range.
    pub fn read_only_field_index(&self) -> usize {
        self.field_count().saturating_sub(1)
    }

    /// Whether the given index points at the Read-Only Mode field. Centralised
    /// so the cycle/value/edit code-paths agree on the same answer regardless
    /// of provider type or auth mode.
    pub fn is_read_only_field(&self, index: usize) -> bool {
        index == self.read_only_field_index()
    }

    /// Returns a mutable reference to the string field at the given index.
    /// Returns None for cycle/toggle fields, read-only fields, and actions.
    pub fn get_field_mut(&mut self, index: usize) -> Option<&mut String> {
        match self.provider_type {
            ProviderType::S3 | ProviderType::Azure => match index {
                0 => Some(&mut self.name),
                2 => Some(self.region.get_or_insert_with(String::new)),
                3 => Some(self.bucket_name.get_or_insert_with(String::new)),
                4 => Some(self.endpoint.get_or_insert_with(String::new)),
                5 => Some(self.access_key_id.get_or_insert_with(String::new)),
                6 => Some(self.secret_access_key.get_or_insert_with(String::new)),
                _ => None,
            },
            ProviderType::GCS => match (self.gcs_auth_mode.clone(), index) {
                (_, 0) => Some(&mut self.name),
                (GcsAuthMode::ServiceAccount, 3) => {
                    Some(self.project.get_or_insert_with(String::new))
                }
                (GcsAuthMode::ServiceAccount, 4) => {
                    Some(self.bucket_name.get_or_insert_with(String::new))
                }
                (GcsAuthMode::ServiceAccount, 5) => {
                    Some(self.credentials_path.get_or_insert_with(String::new))
                }
                // GcloudAdc fields (3 = ADC status, 4 = projects list) are
                // managed through the project picker modal, not free text.
                _ => None,
            },
        }
    }

    /// Whether this field index is a cycle/toggle field (not free-text editable).
    pub fn is_cycle_field(&self, index: usize) -> bool {
        if index == 1 {
            return true;
        } // Type field — always a cycle
        if self.is_read_only_field(index) {
            return true;
        }
        match self.provider_type {
            ProviderType::S3 | ProviderType::Azure => index == 7, // Allow HTTP
            ProviderType::GCS => index == 2,                      // Auth Mode
        }
    }

    /// The editing hint shown when a cycle field is selected.
    pub fn cycle_field_hint(&self, index: usize) -> Option<String> {
        if index == 1 {
            return Some(format!(
                "[ S3 | GCS | Azure ] - Current: {:?}",
                self.provider_type
            ));
        }
        if self.is_read_only_field(index) {
            return Some(format!(
                "[ true | false ] - Current: {} (blocks upload / new folder / delete)",
                self.read_only
            ));
        }
        match (&self.provider_type, index) {
            (ProviderType::S3 | ProviderType::Azure, 7) => Some(format!(
                "[ true | false ] - Current: {}",
                self.allow_http.unwrap_or(false)
            )),
            (ProviderType::GCS, 2) => {
                let current = match self.gcs_auth_mode {
                    GcsAuthMode::ServiceAccount => "Service Account",
                    GcsAuthMode::GcloudAdc => "gcloud ADC",
                };
                Some(format!(
                    "[ Service Account | gcloud ADC ] - Current: {}",
                    current
                ))
            }
            _ => None,
        }
    }

    /// Whether this field is a "read-only / action" field — not editable as
    /// text, not a cycle. The UI surfaces these with a hint instead of an
    /// editing caret. Used by GcloudAdc mode for the ADC status line and
    /// the Selected Projects action.
    pub fn is_action_field(&self, index: usize) -> bool {
        matches!(self.provider_type, ProviderType::GCS)
            && matches!(self.gcs_auth_mode, GcsAuthMode::GcloudAdc)
            && (index == 3 || index == 4)
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if !config_path.exists() {
            return Ok(Config::default());
        }
        let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
        let mut config: Config =
            serde_json::from_str(&content).context("Failed to parse config file")?;
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
