use crate::config::{Config, ProviderConfig, ProviderType, KeymapConfig, SettingsConfig};
use crate::storage::{Bucket, Object, StorageBackend, ProgressCallback};
use object_store::path::Path as ObjectPath;
use crate::storage::s3::S3Storage;
use crate::storage::gcs::GcsStorage;
use ratatui::widgets::{ListState, TableState};
use ratatui::style::{Color, Modifier, Style};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Theme};
use anyhow::{Result};

pub enum InputMode {
    Normal,
    Editing,
    Search,
    DownloadPrompt,
    UploadPrompt,
    DeleteConfirmation,
    CreateFolderPrompt,
    OverwriteConfirmation,
}

#[derive(PartialEq, Eq)]
pub enum Pane {
    Sidebar,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTab {
    Providers,
    Keymap,
    Settings,
    Theme,
}

impl ConfigTab {
    pub const ALL: [ConfigTab; 4] = [ConfigTab::Providers, ConfigTab::Keymap, ConfigTab::Settings, ConfigTab::Theme];

    pub fn label(&self) -> &'static str {
        match self {
            ConfigTab::Providers => "Providers",
            ConfigTab::Keymap => "Keymap",
            ConfigTab::Settings => "Settings",
            ConfigTab::Theme => "Theme",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            ConfigTab::Providers => ConfigTab::Keymap,
            ConfigTab::Keymap => ConfigTab::Settings,
            ConfigTab::Settings => ConfigTab::Theme,
            ConfigTab::Theme => ConfigTab::Providers,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            ConfigTab::Providers => ConfigTab::Theme,
            ConfigTab::Keymap => ConfigTab::Providers,
            ConfigTab::Settings => ConfigTab::Keymap,
            ConfigTab::Theme => ConfigTab::Settings,
        }
    }
}

pub struct ConfigModalState {
    pub active_tab: ConfigTab,
    // Providers tab
    pub providers: Vec<ProviderConfig>,
    pub selected_provider_index: usize,
    pub selected_field_index: usize,
    pub edit_mode: bool,
    // Keymap tab
    pub keymap: KeymapConfig,
    pub selected_keymap_index: usize,
    pub keymap_editing: bool,
    pub keymap_input: String,
    // Settings tab
    pub settings: SettingsConfig,
    pub selected_setting_index: usize,
    // Theme tab
    pub selected_theme_index: usize,
    pub original_theme: String,
}

#[derive(Clone)]
pub enum SidebarItem {
    Provider(usize),
    Bucket { provider_index: usize, name: String, location: Option<String> },
}

#[derive(Clone, PartialEq, Eq)]
pub enum TaskType {
    Upload,
    Download,
    CreateFolder,
    Delete,
}

#[derive(Clone)]
pub struct AppTask {
    pub id: String,
    pub description: String,
    pub task_type: TaskType,
    pub bytes_transferred: u64,
    pub total_size: u64,
    pub start_time: std::time::Instant,
}

#[derive(Clone)]
pub struct FinishedTask {
    pub description: String,
    pub task_type: TaskType,
    pub result: Result<(), String>,
    pub end_time: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictChoice {
    Overwrite,
    Skip,
    Rename,
    OverwriteAll,
    SkipAll,
}

#[derive(Debug, Clone)]
pub enum ConflictOperation {
    Download {
        remote_path: ObjectPath,
        local_path: std::path::PathBuf,
        size: u64,
    },
    Upload {
        local_path: std::path::PathBuf,
        remote_path: ObjectPath,
        size: u64,
    },
}

#[derive(Debug, Clone)]
pub struct PendingConflict {
    pub source_name: String,
    pub target_display: String,
    pub operation: ConflictOperation,
}

pub struct ConflictResolutionState {
    pub conflicts: Vec<PendingConflict>,
    pub current_index: usize,
    pub resolved: Vec<(PendingConflict, ConflictChoice)>,
    pub no_conflict_items: Vec<ConflictOperation>,
    pub selected_option: usize,
    pub bucket: String,
    pub provider_idx: usize,
    pub existing_remote_keys: Option<HashSet<String>>,
}

pub enum AppEvent {
    TaskProgress { id: String, bytes: u64 },
    TaskFinished { id: String, result: Result<(), String> },
    RequestRefresh,
    ConnectCompleted {
        provider_index: usize,
        provider_name: String,
        result: Result<Vec<crate::storage::Bucket>, String>,
    },
    ObjectsLoadCompleted {
        path: Option<ObjectPath>,
        set_current_path: bool,
        highlight_path: Option<ObjectPath>,
        result: Result<Vec<Object>, String>,
    },
}

pub enum PendingAction {
    ExpandProvider {
        provider_index: usize,
        provider_name: String,
    },
    LoadObjects {
        provider_index: usize,
        bucket: String,
        path: Option<ObjectPath>,
        set_current_path: bool,
        highlight_path: Option<ObjectPath>,
    },
}

pub struct App {
    pub config: Config,
    pub storage_providers: Vec<Box<dyn StorageBackend>>,
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub show_config_modal: bool,
    pub config_modal_state: Option<ConfigModalState>,
    pub show_help: bool,
    pub error_message: Option<String>,
    pub save_message: Option<String>,
    pub save_message_time: Option<std::time::Instant>,
    pub active_provider_index: Option<usize>,
    pub pending_delete_index: Option<usize>,

    // Navigation State
    pub active_pane: Pane,
    pub sidebar_state: ListState,
    pub main_list_state: TableState,
    pub all_buckets: Vec<Bucket>,
    pub expanded_providers: HashSet<usize>,
    pub activated_providers: HashSet<usize>,
    pub objects: Vec<Object>,
    pub selected_paths: HashSet<object_store::path::Path>,
    pub current_bucket: Option<String>,
    pub current_path: Option<object_store::path::Path>,
    pub preview_data: Option<bytes::Bytes>,
    pub preview_enabled: bool,
    pub search_query: String,

    // Task State
    pub active_tasks: Vec<AppTask>,
    pub last_finished_task: Option<FinishedTask>,
    pub session_tasks: Vec<FinishedTask>,
    pub prompt_input: String,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,

    // File Explorer for Upload/Download
    pub file_explorer: FileExplorer,
    pub file_explorer_filter: String,
    pub file_explorer_search_active: bool,

    pub copy_button_area: Option<ratatui::layout::Rect>,
    pub error_copy_area: Option<ratatui::layout::Rect>,
    pub task_copy_area: Option<ratatui::layout::Rect>,

    // Conflict Resolution
    pub conflict_state: Option<ConflictResolutionState>,

    // Loading State
    pub loading_message: Option<String>,
    pub pending_action: Option<PendingAction>,
}

impl App {
    pub async fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let config = Config::load().unwrap_or_default();

        let home_dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let theme = Theme::default()
            .with_highlight_symbol("> ")
            .with_highlight_item_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            .with_highlight_dir_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

        let file_explorer = FileExplorerBuilder::default()
            .working_dir(home_dir)
            .theme(theme)
            .build()
            .expect("Failed to initialize file explorer");

        let mut app = Self {
            config,
            storage_providers: Vec::new(),
            should_quit: false,
            input_mode: InputMode::Normal,
            show_config_modal: false,
            config_modal_state: None,
            show_help: false,
            error_message: None,
            save_message: None,
            save_message_time: None,
            active_provider_index: None,
            pending_delete_index: None,
            active_pane: Pane::Sidebar,
            sidebar_state: ListState::default(),
            main_list_state: TableState::default(),
            all_buckets: Vec::new(),
            expanded_providers: HashSet::new(),
            activated_providers: HashSet::new(),
            objects: Vec::new(),
            selected_paths: HashSet::new(),
            current_bucket: None,
            current_path: None,
            preview_data: None,
            preview_enabled: false,
            search_query: String::new(),
            active_tasks: Vec::new(),
            last_finished_task: None,
            session_tasks: Vec::new(),
            prompt_input: String::new(),
            event_tx,
            event_rx,
            file_explorer,
            file_explorer_filter: String::new(),
            file_explorer_search_active: false,
            copy_button_area: None,
            error_copy_area: None,
            task_copy_area: None,
            conflict_state: None,
            loading_message: None,
            pending_action: None,
        };

        app.refresh_storage_providers().await;
        
        if !app.sidebar_items().is_empty() {
            app.sidebar_state.select(Some(0));
        }
        
        app
    }

    pub fn filtered_objects(&self) -> Vec<&Object> {
        if self.search_query.is_empty() {
            return self.objects.iter().collect();
        }
        
        let query = self.search_query.to_lowercase();
        self.objects.iter()
            .filter(|obj| {
                let filename = obj.path.filename().unwrap_or_else(|| obj.path.as_ref()).to_lowercase();
                filename.contains(&query)
            })
            .collect()
    }

    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = Vec::new();
        for i in 0..self.storage_providers.len() {
            items.push(SidebarItem::Provider(i));
            if self.expanded_providers.contains(&i) {
                for bucket in &self.all_buckets {
                    if bucket.provider_index == i {
                        items.push(SidebarItem::Bucket {
                            provider_index: i,
                            name: bucket.name.clone(),
                            location: bucket.location.clone(),
                        });
                    }
                }
            }
        }
        items
    }

    pub async fn refresh_storage_providers(&mut self) {
        let mut storage_providers: Vec<Box<dyn StorageBackend>> = Vec::new();
        let mut project_backfills: Vec<(usize, String)> = Vec::new();
        for (i, provider_config) in self.config.providers.iter().enumerate() {
            if !provider_config.enabled {
                continue;
            }
            match provider_config.provider_type {
                ProviderType::S3 => {
                    match S3Storage::new(provider_config.clone(), i).await {
                        Ok(storage) => storage_providers.push(Box::new(storage)),
                        Err(e) => tracing::error!("Failed to initialize S3 provider {}: {}", provider_config.name, e),
                    }
                }
                ProviderType::GCS => {
                    match GcsStorage::new(provider_config.clone(), i).await {
                        Ok(storage) => {
                            if provider_config.project.as_ref().map_or(true, |p| p.trim().is_empty()) {
                                if let Some(project) = storage.detected_project_id() {
                                    project_backfills.push((i, project.to_string()));
                                }
                            }
                            storage_providers.push(Box::new(storage));
                        }
                        Err(e) => tracing::error!("Failed to initialize GCS provider {}: {}", provider_config.name, e),
                    }
                }
                _ => {}
            }
        }
        for (i, project) in project_backfills {
            self.config.providers[i].project = Some(project);
        }
        self.storage_providers = storage_providers;

        // Fetch all buckets
        let mut all_buckets = Vec::new();
        for provider in &self.storage_providers {
            match provider.list_buckets().await {
                Ok(mut buckets) => all_buckets.append(&mut buckets),
                Err(e) => tracing::error!("Failed to list buckets for provider {}: {}", provider.name(), e),
            }
        }
        self.all_buckets = all_buckets;

        // Reset sidebar state if needed
        if self.sidebar_state.selected().is_none() && !self.sidebar_items().is_empty() {
            self.sidebar_state.select(Some(0));
        }
        self.update_preview().await;
    }

    fn apply_file_explorer_filter(&mut self) {
        let cwd = self.file_explorer.cwd().to_owned();
        if self.file_explorer_filter.is_empty() {
            let _ = self.file_explorer.remove_filter_map();
        } else {
            let query = self.file_explorer_filter.to_lowercase();
            let _ = self.file_explorer.set_filter_map(move |file| {
                if file.name == "../" || file.name.to_lowercase().contains(&query) {
                    Some(file)
                } else {
                    None
                }
            });
        }
        let _ = self.file_explorer.set_cwd(cwd);
    }

    fn reset_file_explorer_filter(&mut self) {
        self.file_explorer_filter.clear();
        self.file_explorer_search_active = false;
        let cwd = self.file_explorer.cwd().to_owned();
        let _ = self.file_explorer.remove_filter_map();
        let _ = self.file_explorer.set_cwd(cwd);
    }

    pub fn set_error(&mut self, msg: String) {
        tracing::error!("{}", msg);
        self.error_message = Some(msg);
    }

    pub async fn update_tasks(&mut self) {
        // Auto-dismiss save message after 2 seconds
        if let Some(start_time) = self.save_message_time {
            if start_time.elapsed().as_secs() >= 2 {
                self.save_message = None;
                self.save_message_time = None;
            }
        }

        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::TaskProgress { id, bytes } => {
                    if let Some(task) = self.active_tasks.iter_mut().find(|t| t.id == id) {
                        task.bytes_transferred += bytes;
                    }
                }
                AppEvent::TaskFinished { id, result } => {
                    if let Some(task) = self.active_tasks.iter().find(|t| t.id == id) {
                        let finished = FinishedTask {
                            description: task.description.clone(),
                            task_type: task.task_type.clone(),
                            result: result.clone(),
                            end_time: std::time::Instant::now(),
                        };
                        self.last_finished_task = Some(finished.clone());
                        self.session_tasks.push(finished);
                        
                        // Auto-refresh for certain operations
                        if (task.task_type == TaskType::Upload || 
                            task.task_type == TaskType::CreateFolder || 
                            task.task_type == TaskType::Delete) && result.is_ok() {
                            let _ = self.event_tx.send(AppEvent::RequestRefresh);
                        }
                    }
                    self.active_tasks.retain(|t| t.id != id);
                    if let Err(e) = result {
                        self.set_error(format!("Operation failed: {}", e));
                    }
                }
                AppEvent::RequestRefresh => {
                    self.refresh_objects().await;
                }
                AppEvent::ConnectCompleted { provider_index, provider_name, result } => {
                    match result {
                        Ok(buckets) => {
                            self.all_buckets.retain(|b| b.provider_index != provider_index);
                            self.all_buckets.extend(buckets);
                            self.activated_providers.insert(provider_index);
                            self.active_provider_index = Some(provider_index);
                            self.expanded_providers.insert(provider_index);

                            let items = self.sidebar_items();
                            let first_bucket = items.iter().enumerate().find_map(|(idx, item)| {
                                if let SidebarItem::Bucket { provider_index: pi, name, .. } = item {
                                    if *pi == provider_index {
                                        return Some((idx, name.clone()));
                                    }
                                }
                                None
                            });

                            if let Some((item_idx, name)) = first_bucket {
                                self.sidebar_state.select(Some(item_idx));
                                self.current_bucket = Some(name.clone());
                                self.current_path = None;
                                self.search_query.clear();
                                self.save_message = Some(format!("Connected to '{}'", provider_name));
                                self.save_message_time = Some(std::time::Instant::now());
                                // Chain: load objects for the auto-selected bucket
                                self.loading_message = Some("Loading...".to_string());
                                self.pending_action = Some(PendingAction::LoadObjects {
                                    provider_index,
                                    bucket: name,
                                    path: None,
                                    set_current_path: false,
                                    highlight_path: None,
                                });
                            } else {
                                self.save_message = Some(format!("Connected to '{}', but no buckets found.", provider_name));
                                self.save_message_time = Some(std::time::Instant::now());
                                self.loading_message = None;
                            }
                        }
                        Err(e) => {
                            self.set_error(format!("Connection to '{}' failed: {}", provider_name, e));
                            self.loading_message = None;
                        }
                    }
                }
                AppEvent::ObjectsLoadCompleted { path, set_current_path, highlight_path, result } => {
                    match result {
                        Ok(objects) => {
                            if set_current_path {
                                self.current_path = path;
                            }
                            self.objects = objects;
                            self.search_query.clear();

                            let select_index = highlight_path
                                .and_then(|hp| self.objects.iter().position(|obj| obj.path == hp))
                                .unwrap_or(0);

                            if !self.objects.is_empty() {
                                self.main_list_state.select(Some(select_index));
                            } else {
                                self.main_list_state.select(None);
                            }
                            if self.preview_enabled {
                                self.update_preview().await;
                            }
                        }
                        Err(e) => self.set_error(format!("Failed to list objects: {}", e)),
                    }
                    self.loading_message = None;
                }
            }
        }
    }

    pub async fn refresh_objects(&mut self) {
        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let provider_idx = match self.active_provider_index {
            Some(idx) => idx,
            None => return,
        };

        if let Some(provider) = self.storage_providers.get(provider_idx) {
            match provider.list_objects(&bucket, self.current_path.as_ref()).await {
                Ok(objects) => {
                    self.objects = objects;
                    if !self.objects.is_empty() {
                        if self.main_list_state.selected().is_none() {
                            self.main_list_state.select(Some(0));
                        }
                    } else {
                        self.main_list_state.select(None);
                    }
                    self.update_preview().await;
                }
                Err(e) => self.set_error(format!("Failed to refresh objects: {}", e)),
            }
        }
    }

    pub async fn on_event(&mut self, event: &crossterm::event::Event) {
        match event {
            crossterm::event::Event::Key(key) => {
                self.handle_key(key.code, key.modifiers, event).await;
            }
            crossterm::event::Event::Mouse(mouse) => {
                if mouse.kind == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) {
                    if let Some(area) = self.copy_button_area {
                        if mouse.column >= area.x && mouse.column < area.x + area.width &&
                           mouse.row >= area.y && mouse.row < area.y + area.height {
                            self.copy_path_to_clipboard();
                        }
                    }
                    if let Some(area) = self.error_copy_area {
                        if self.error_message.is_some() &&
                           mouse.column >= area.x && mouse.column < area.x + area.width &&
                           mouse.row >= area.y && mouse.row < area.y + area.height {
                            self.copy_error_to_clipboard();
                        }
                    }
                    if let Some(area) = self.task_copy_area {
                        if mouse.column >= area.x && mouse.column < area.x + area.width &&
                           mouse.row >= area.y && mouse.row < area.y + area.height {
                            self.copy_task_to_clipboard();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn copy_path_to_clipboard(&mut self) {
        let path = self.get_copy_path();
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(path) {
                    self.set_error(format!("Failed to copy to clipboard: {}", e));
                } else {
                    self.save_message = Some("Path copied to clipboard!".to_string());
                    self.save_message_time = Some(std::time::Instant::now());
                }
            }
            Err(e) => self.set_error(format!("Failed to initialize clipboard: {}", e)),
        }
    }

    fn copy_error_to_clipboard(&mut self) {
        if let Some(error) = &self.error_message {
            let msg = error.clone();
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(e) = clipboard.set_text(msg) {
                        self.set_error(format!("Failed to copy to clipboard: {}", e));
                    } else {
                        self.error_message = None;
                        self.save_message = Some("Error message copied to clipboard!".to_string());
                        self.save_message_time = Some(std::time::Instant::now());
                    }
                }
                Err(e) => self.set_error(format!("Failed to initialize clipboard: {}", e)),
            }
        }
    }

    fn copy_task_to_clipboard(&mut self) {
        let status_text = if let Some(last_task) = self.active_tasks.last() {
            format!("[{}] {}", match last_task.task_type {
                TaskType::Upload => "Upload",
                TaskType::Download => "Download",
                TaskType::CreateFolder => "New Folder",
                TaskType::Delete => "Delete",
            }, last_task.description)
        } else if let Some(finished) = &self.last_finished_task {
            format!("[{}] {}: {}", match finished.task_type {
                TaskType::Upload => "Upload",
                TaskType::Download => "Download",
                TaskType::CreateFolder => "New Folder",
                TaskType::Delete => "Delete",
            }, finished.description, match &finished.result { Ok(_) => "Done", Err(e) => e })
        } else {
            "Ready".to_string()
        };

        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(status_text) {
                    self.set_error(format!("Failed to copy task status: {}", e));
                } else {
                    self.save_message = Some("Task status copied to clipboard!".to_string());
                    self.save_message_time = Some(std::time::Instant::now());
                }
            }
            Err(e) => self.set_error(format!("Failed to initialize clipboard: {}", e)),
        }
    }

    pub fn get_copy_path(&self) -> String {
        if let Some(idx) = self.active_provider_index {
            if let Some(provider) = self.storage_providers.get(idx) {
                if provider.name().to_lowercase().contains("minio") {
                    if let Some(config) = self.config.providers.get(idx) {
                        if let Some(endpoint) = &config.endpoint {
                            let bucket = self.current_bucket.as_deref().unwrap_or("");
                            let path = self.current_path.as_ref().map(|p| p.to_string()).unwrap_or_default();
                            
                            let mut base = endpoint.clone();
                            if !base.ends_with('/') {
                                base.push('/');
                            }
                            base.push_str("browser/");
                            if !bucket.is_empty() {
                                base.push_str(bucket);
                                if !path.is_empty() {
                                    base.push('/');
                                    base.push_str(&path);
                                    // Minio browser expects folders to end with %2F
                                    base.push_str("%2F");
                                }
                            }
                            return base;
                        }
                    }
                }
            }
        }
        self.get_full_remote_path()
    }

    pub fn get_full_remote_path(&self) -> String {
        let protocol = match self.active_provider_index {
            Some(idx) => {
                if let Some(provider) = self.storage_providers.get(idx) {
                    match provider.provider_type() {
                        crate::config::ProviderType::S3 => {
                            if provider.name().to_lowercase().contains("minio") {
                                "minio"
                            } else {
                                "s3"
                            }
                        }
                        crate::config::ProviderType::GCS => "gs",
                        crate::config::ProviderType::Azure => "azure",
                    }
                } else {
                    "unknown"
                }
            }
            None => "unknown",
        };

        let bucket = self.current_bucket.as_deref().unwrap_or("");
        let path = self.current_path.as_ref().map(|p| p.to_string()).unwrap_or_default();
        
        if bucket.is_empty() {
            return format!("{}://", protocol);
        }
        
        if path.is_empty() {
            return format!("{}://{}", protocol, bucket);
        }
        
        format!("{}://{}/{}", protocol, bucket, path)
    }

    async fn handle_key(&mut self, key_code: crossterm::event::KeyCode, key_modifiers: crossterm::event::KeyModifiers, event: &crossterm::event::Event) {
        if self.pending_delete_index.is_some() {
            match key_code {
                crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                    if let Some(index) = self.pending_delete_index {
                        if let Some(state) = &mut self.config_modal_state {
                             state.providers.remove(index);
                             self.config.providers = state.providers.clone();
                             if let Err(e) = self.config.save() {
                                 self.set_error(format!("Failed to save config: {}", e));
                             } else {
                                 self.refresh_storage_providers().await;
                                 self.save_message = Some("Provider deleted successfully.".to_string());
                                 self.save_message_time = Some(std::time::Instant::now());
                             }
                        }
                    }
                    self.pending_delete_index = None;
                    return;
                }
                crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') | crossterm::event::KeyCode::Esc => {
                    self.pending_delete_index = None;
                    return;
                }
                _ => {}
            }
        }

        if self.error_message.is_some() || self.save_message.is_some() {
             match key_code {
                crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Enter => {
                    self.error_message = None;
                    self.save_message = None;
                    self.save_message_time = None;
                    return;
                }
                _ => {
                    // Don't auto-dismiss error on key press, but maybe save_message?
                    // Actually, keep current behavior but clear time.
                    if self.save_message.is_some() {
                        self.save_message = None;
                        self.save_message_time = None;
                    }
                }
            }
        }

        match self.input_mode {
            InputMode::Normal => match key_code {
                crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc => {
                    if self.show_config_modal {
                        // Revert theme if not saved
                        if let Some(state) = &self.config_modal_state {
                            self.config.theme = state.original_theme.clone();
                        }
                        self.show_config_modal = false;
                        self.config_modal_state = None;
                    } else if self.show_help {
                        self.show_help = false;
                    } else {
                        self.should_quit = true;
                    }
                }
                crossterm::event::KeyCode::Char('?') => {
                    self.show_help = !self.show_help;
                }
                crossterm::event::KeyCode::Char('/') => {
                    if self.active_pane == Pane::Main && !self.objects.is_empty() {
                        self.input_mode = InputMode::Search;
                    }
                }
                crossterm::event::KeyCode::Char('y') => {
                    if self.active_pane == Pane::Main {
                        self.copy_path_to_clipboard();
                    }
                }
                crossterm::event::KeyCode::Char('n') => {
                    if self.active_pane == Pane::Main && self.current_bucket.is_some() {
                        self.input_mode = InputMode::CreateFolderPrompt;
                        self.prompt_input = String::new();
                    }
                }
                crossterm::event::KeyCode::Char('d') => {
                    if self.active_pane == Pane::Main && (!self.selected_paths.is_empty() || self.main_list_state.selected().is_some()) {
                        self.input_mode = InputMode::DownloadPrompt;
                        self.prompt_input = String::new();
                        let home_dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                        let _ = self.file_explorer.set_cwd(home_dir);
                    }
                }
                crossterm::event::KeyCode::Char('u') => {
                    if self.active_pane == Pane::Main && self.current_bucket.is_some() {
                        self.input_mode = InputMode::UploadPrompt;
                        self.prompt_input = String::new();
                        // Reset file explorer to home directory
                        let home_dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                        let _ = self.file_explorer.set_cwd(home_dir);
                    }
                }
                crossterm::event::KeyCode::Char('x') | crossterm::event::KeyCode::Delete => {
                    if self.active_pane == Pane::Main && (!self.selected_paths.is_empty() || self.main_list_state.selected().is_some()) {
                        self.input_mode = InputMode::DeleteConfirmation;
                    }
                }
                crossterm::event::KeyCode::Char(' ') => {
                    if self.active_pane == Pane::Main {
                        if let Some(index) = self.main_list_state.selected() {
                            let filtered = self.filtered_objects();
                            if let Some(obj) = filtered.get(index) {
                                let path = obj.path.clone();
                                if self.selected_paths.contains(&path) {
                                    self.selected_paths.remove(&path);
                                } else {
                                    self.selected_paths.insert(path);
                                }
                            }
                        }
                    }
                }
                crossterm::event::KeyCode::Char('p') if !self.show_config_modal => {
                    self.preview_enabled = !self.preview_enabled;
                    if !self.preview_enabled {
                        self.preview_data = None;
                    } else {
                        self.update_preview().await;
                    }
                }
                crossterm::event::KeyCode::Char('c') => {
                    self.show_config_modal = !self.show_config_modal;
                    if self.show_config_modal {
                        self.config_modal_state = Some(ConfigModalState {
                            active_tab: ConfigTab::Providers,
                            providers: self.config.providers.clone(),
                            selected_provider_index: 0,
                            selected_field_index: 0,
                            edit_mode: false,
                            keymap: self.config.keymap.clone(),
                            selected_keymap_index: 0,
                            keymap_editing: false,
                            keymap_input: String::new(),
                            settings: self.config.settings.clone(),
                            selected_setting_index: 0,
                            selected_theme_index: crate::theme::theme_index(&self.config.theme),
                            original_theme: self.config.theme.clone(),
                        });
                        if self.config_modal_state.as_ref().unwrap().providers.is_empty() {
                            self.config_modal_state.as_mut().unwrap().providers.push(ProviderConfig::default());
                        }
                    } else {
                        // Revert theme if not saved
                        if let Some(state) = &self.config_modal_state {
                            self.config.theme = state.original_theme.clone();
                        }
                        self.config_modal_state = None;
                    }
                }
                crossterm::event::KeyCode::Char('D') if self.show_config_modal => {
                    if let Some(state) = &self.config_modal_state {
                        if state.active_tab == ConfigTab::Providers && !state.providers.is_empty() {
                             self.pending_delete_index = Some(state.selected_provider_index);
                        }
                    }
                }
                crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            match state.active_tab {
                                ConfigTab::Providers => {
                                    if state.selected_provider_index < state.providers.len().saturating_sub(1) {
                                        state.selected_provider_index += 1;
                                    }
                                }
                                ConfigTab::Keymap => {
                                    if state.selected_keymap_index < KeymapConfig::FIELD_COUNT.saturating_sub(1) {
                                        state.selected_keymap_index += 1;
                                    }
                                }
                                ConfigTab::Settings => {
                                    if state.selected_setting_index < SettingsConfig::FIELD_COUNT.saturating_sub(1) {
                                        state.selected_setting_index += 1;
                                    }
                                }
                                ConfigTab::Theme => {
                                    if state.selected_theme_index < crate::theme::BUILTIN_THEMES.len().saturating_sub(1) {
                                        state.selected_theme_index += 1;
                                        self.config.theme = crate::theme::BUILTIN_THEMES[state.selected_theme_index].name.to_string();
                                    }
                                }
                            }
                        }
                    } else {
                        self.move_down().await;
                    }
                }
                crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            match state.active_tab {
                                ConfigTab::Providers => {
                                    if state.selected_provider_index > 0 {
                                        state.selected_provider_index -= 1;
                                    }
                                }
                                ConfigTab::Keymap => {
                                    if state.selected_keymap_index > 0 {
                                        state.selected_keymap_index -= 1;
                                    }
                                }
                                ConfigTab::Settings => {
                                    if state.selected_setting_index > 0 {
                                        state.selected_setting_index -= 1;
                                    }
                                }
                                ConfigTab::Theme => {
                                    if state.selected_theme_index > 0 {
                                        state.selected_theme_index -= 1;
                                        self.config.theme = crate::theme::BUILTIN_THEMES[state.selected_theme_index].name.to_string();
                                    }
                                }
                            }
                        }
                    } else {
                        self.move_up().await;
                    }
                }
                crossterm::event::KeyCode::Char('a') if self.show_config_modal => {
                    if let Some(state) = &mut self.config_modal_state {
                        if state.active_tab == ConfigTab::Providers {
                            state.providers.push(ProviderConfig::default());
                            state.selected_provider_index = state.providers.len() - 1;
                        }
                    }
                }
                crossterm::event::KeyCode::Char('e') if self.show_config_modal => {
                    if let Some(state) = &mut self.config_modal_state {
                        match state.active_tab {
                            ConfigTab::Providers => {
                                if !state.providers.is_empty() {
                                    self.input_mode = InputMode::Editing;
                                    state.edit_mode = true;
                                }
                            }
                            ConfigTab::Keymap => {
                                state.keymap_editing = true;
                                state.keymap_input = state.keymap.get_field(state.selected_keymap_index).to_string();
                                self.input_mode = InputMode::Editing;
                            }
                            ConfigTab::Settings => {
                                // 'e' for editable fields (preview_max_bytes = 2, log_file_path = 4)
                                if state.selected_setting_index == 2 || state.selected_setting_index == 4 {
                                    self.input_mode = InputMode::Editing;
                                }
                            }
                            ConfigTab::Theme => {} // themes are selected with j/k, not edited
                        }
                    }
                }
                crossterm::event::KeyCode::Char('s') if self.show_config_modal => {
                    if let Some(state) = &self.config_modal_state {
                        self.config.providers = state.providers.clone();
                        self.config.keymap = state.keymap.clone();
                        self.config.settings = state.settings.clone();
                        match self.config.save() {
                            Ok(_) => {
                                // Check if the active provider was disabled
                                let active_disabled = self.active_provider_index
                                    .and_then(|idx| self.config.providers.get(idx))
                                    .is_some_and(|p| !p.enabled);
                                self.refresh_storage_providers().await;
                                if active_disabled {
                                    self.active_provider_index = None;
                                    self.current_bucket = None;
                                    self.current_path = None;
                                    self.objects.clear();
                                    self.selected_paths.clear();
                                    self.search_query.clear();
                                    self.main_list_state.select(None);
                                    self.preview_data = None;
                                }
                                if let Some(ms) = &mut self.config_modal_state {
                                    ms.original_theme = self.config.theme.clone();
                                }
                                let config_path = dirs::home_dir().unwrap().join(".config").join("stuie").join("config.json");
                                self.save_message = Some(format!("Configuration saved to {}!", config_path.display()));
                                self.save_message_time = Some(std::time::Instant::now());
                            }
                            Err(e) => self.set_error(format!("Failed to save config: {}", e)),
                        }
                    }
                }
                crossterm::event::KeyCode::Tab | crossterm::event::KeyCode::BackTab => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            state.active_tab = if key_code == crossterm::event::KeyCode::Tab {
                                state.active_tab.next()
                            } else {
                                state.active_tab.prev()
                            };
                        }
                    } else {
                        self.toggle_pane();
                    }
                }
                crossterm::event::KeyCode::Enter => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            match state.active_tab {
                                ConfigTab::Providers => {
                                    if let Some(provider) = state.providers.get_mut(state.selected_provider_index) {
                                        provider.enabled = !provider.enabled;
                                    }
                                }
                                ConfigTab::Keymap => {
                                    state.keymap_editing = true;
                                    state.keymap_input = state.keymap.get_field(state.selected_keymap_index).to_string();
                                    self.input_mode = InputMode::Editing;
                                }
                                ConfigTab::Settings => {
                                    match state.selected_setting_index {
                                        0 => state.settings.show_icons = !state.settings.show_icons,
                                        1 => state.settings.confirm_delete = !state.settings.confirm_delete,
                                        2 => self.input_mode = InputMode::Editing,
                                        3 => state.settings.log_to_file = !state.settings.log_to_file,
                                        4 => self.input_mode = InputMode::Editing,
                                        _ => {}
                                    }
                                }
                                ConfigTab::Theme => {} // already applied live via j/k
                            }
                        }
                    } else {
                        self.on_enter();
                    }
                }
                crossterm::event::KeyCode::Backspace => {
                    if !self.show_config_modal {
                        self.on_back();
                    }
                }
                _ => {}
            },
            InputMode::Editing => {
                let active_tab = self.config_modal_state.as_ref().map(|s| s.active_tab);
                match active_tab {
                    Some(ConfigTab::Keymap) => match key_code {
                        crossterm::event::KeyCode::Esc => {
                            if let Some(state) = &mut self.config_modal_state {
                                state.keymap_editing = false;
                                state.keymap_input.clear();
                            }
                            self.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Enter => {
                            if let Some(state) = &mut self.config_modal_state {
                                let value = state.keymap_input.clone();
                                if !value.is_empty() {
                                    state.keymap.set_field(state.selected_keymap_index, value);
                                }
                                state.keymap_editing = false;
                                state.keymap_input.clear();
                            }
                            self.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if let Some(state) = &mut self.config_modal_state {
                                if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                                    state.keymap_input.clear();
                                } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                    delete_previous_word(&mut state.keymap_input);
                                } else {
                                    state.keymap_input.pop();
                                }
                            }
                        }
                        crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                delete_previous_word(&mut state.keymap_input);
                            }
                        }
                        crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                state.keymap_input.clear();
                            }
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            if let Some(state) = &mut self.config_modal_state {
                                state.keymap_input.push(c);
                            }
                        }
                        _ => {}
                    },
                    Some(ConfigTab::Settings) => match key_code {
                        crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Enter => {
                            self.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if let Some(state) = &mut self.config_modal_state {
                                match state.selected_setting_index {
                                    2 => {
                                        let s = &mut state.settings.preview_max_bytes;
                                        *s /= 10;
                                    }
                                    4 => {
                                        if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                                            state.settings.log_file_path.clear();
                                        } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                            delete_previous_word(&mut state.settings.log_file_path);
                                        } else {
                                            state.settings.log_file_path.pop();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                if state.selected_setting_index == 4 {
                                    delete_previous_word(&mut state.settings.log_file_path);
                                }
                            }
                        }
                        crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                if state.selected_setting_index == 4 {
                                    state.settings.log_file_path.clear();
                                }
                            }
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            if let Some(state) = &mut self.config_modal_state {
                                match state.selected_setting_index {
                                    2 if c.is_ascii_digit() => {
                                        let s = &mut state.settings.preview_max_bytes;
                                        *s = s.saturating_mul(10).saturating_add(c.to_digit(10).unwrap_or(0) as usize);
                                    }
                                    4 => {
                                        state.settings.log_file_path.push(c);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    },
                    _ => match key_code {
                        // Providers editing (existing logic)
                        crossterm::event::KeyCode::Esc => {
                            self.input_mode = InputMode::Normal;
                            if let Some(state) = &mut self.config_modal_state {
                                state.edit_mode = false;
                            }
                        }
                        crossterm::event::KeyCode::Tab | crossterm::event::KeyCode::Down => {
                            if let Some(state) = &mut self.config_modal_state {
                                let count = state.providers[state.selected_provider_index].provider_type.field_count();
                                state.selected_field_index = (state.selected_field_index + 1) % count;
                            }
                        }
                        crossterm::event::KeyCode::BackTab | crossterm::event::KeyCode::Up => {
                            if let Some(state) = &mut self.config_modal_state {
                                let count = state.providers[state.selected_provider_index].provider_type.field_count();
                                if state.selected_field_index == 0 {
                                    state.selected_field_index = count - 1;
                                } else {
                                    state.selected_field_index -= 1;
                                }
                            }
                        }
                        crossterm::event::KeyCode::Right => {
                            if let Some(state) = &mut self.config_modal_state {
                                let provider = &mut state.providers[state.selected_provider_index];
                                if state.selected_field_index == 1 {
                                     provider.provider_type = match provider.provider_type {
                                         ProviderType::S3 => ProviderType::GCS,
                                         ProviderType::GCS => ProviderType::Azure,
                                         ProviderType::Azure => ProviderType::S3,
                                     };
                                } else if provider.is_cycle_field(state.selected_field_index) {
                                     match (&provider.provider_type, state.selected_field_index) {
                                         (ProviderType::S3 | ProviderType::Azure, 7) => {
                                             provider.allow_http = Some(!provider.allow_http.unwrap_or(false));
                                         }
                                         _ => {}
                                     }
                                }
                            }
                        }
                        crossterm::event::KeyCode::Left => {
                            if let Some(state) = &mut self.config_modal_state {
                                let provider = &mut state.providers[state.selected_provider_index];
                                if state.selected_field_index == 1 {
                                     provider.provider_type = match provider.provider_type {
                                         ProviderType::S3 => ProviderType::Azure,
                                         ProviderType::GCS => ProviderType::S3,
                                         ProviderType::Azure => ProviderType::GCS,
                                     };
                                } else if provider.is_cycle_field(state.selected_field_index) {
                                     match (&provider.provider_type, state.selected_field_index) {
                                         (ProviderType::S3 | ProviderType::Azure, 7) => {
                                             provider.allow_http = Some(!provider.allow_http.unwrap_or(false));
                                         }
                                         _ => {}
                                     }
                                }
                            }
                        }
                        crossterm::event::KeyCode::Enter => {
                            self.input_mode = InputMode::Normal;
                            if let Some(state) = &mut self.config_modal_state {
                                state.edit_mode = false;
                            }
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if let Some(state) = &mut self.config_modal_state {
                                if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                                    if let Some(field) = get_editing_field_mut(state) { field.clear(); }
                                } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                    if let Some(field) = get_editing_field_mut(state) { delete_previous_word(field); }
                                } else if let Some(field) = get_editing_field_mut(state) {
                                    field.pop();
                                }
                            }
                        }
                        crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                if let Some(field) = get_editing_field_mut(state) { delete_previous_word(field); }
                            }
                        }
                        crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            if let Some(state) = &mut self.config_modal_state {
                                if let Some(field) = get_editing_field_mut(state) { field.clear(); }
                            }
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            if let Some(state) = &mut self.config_modal_state {
                                if let Some(field) = get_editing_field_mut(state) {
                                    field.push(c);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            InputMode::Search => match key_code {
                crossterm::event::KeyCode::Esc => {
                    self.search_query.clear();
                    self.input_mode = InputMode::Normal;
                    self.update_preview().await;
                }
                crossterm::event::KeyCode::Enter => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Backspace => {
                    if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                        self.search_query.clear();
                    } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                        delete_previous_word(&mut self.search_query);
                    } else {
                        self.search_query.pop();
                    }
                    self.main_list_state.select(Some(0));
                    self.update_preview().await;
                }
                crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    delete_previous_word(&mut self.search_query);
                    self.main_list_state.select(Some(0));
                    self.update_preview().await;
                }
                crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    self.search_query.clear();
                    self.main_list_state.select(Some(0));
                    self.update_preview().await;
                }
                crossterm::event::KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.main_list_state.select(Some(0));
                    self.update_preview().await;
                }
                _ => {}
            }
            InputMode::DownloadPrompt => {
                if self.file_explorer_search_active {
                    match key_code {
                        crossterm::event::KeyCode::Esc => {
                            self.reset_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Enter => {
                            self.file_explorer_search_active = false;
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                                self.file_explorer_filter.clear();
                            } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                delete_previous_word(&mut self.file_explorer_filter);
                            } else {
                                self.file_explorer_filter.pop();
                            }
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            delete_previous_word(&mut self.file_explorer_filter);
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            self.file_explorer_filter.clear();
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            self.file_explorer_filter.push(c);
                            self.apply_file_explorer_filter();
                        }
                        _ => {}
                    }
                } else {
                    match key_code {
                        crossterm::event::KeyCode::Esc => {
                            self.reset_file_explorer_filter();
                            self.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Char('/') => {
                            self.file_explorer_search_active = true;
                        }
                        crossterm::event::KeyCode::Enter => {
                            let current = self.file_explorer.current().path.to_owned();
                            if current.is_dir() {
                                self.prompt_input = current.to_string_lossy().to_string();
                                self.reset_file_explorer_filter();
                                self.start_download().await;
                            } else {
                                if let Some(parent) = current.parent() {
                                    self.prompt_input = parent.to_string_lossy().to_string();
                                    self.reset_file_explorer_filter();
                                    self.start_download().await;
                                } else {
                                    let _ = self.file_explorer.handle(event);
                                }
                            }
                        }
                        _ => {
                            let _ = self.file_explorer.handle(event);
                        }
                    }
                }
            }
            InputMode::UploadPrompt => {
                if self.file_explorer_search_active {
                    match key_code {
                        crossterm::event::KeyCode::Esc => {
                            self.reset_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Enter => {
                            self.file_explorer_search_active = false;
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                                self.file_explorer_filter.clear();
                            } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                                delete_previous_word(&mut self.file_explorer_filter);
                            } else {
                                self.file_explorer_filter.pop();
                            }
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            delete_previous_word(&mut self.file_explorer_filter);
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            self.file_explorer_filter.clear();
                            self.apply_file_explorer_filter();
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            self.file_explorer_filter.push(c);
                            self.apply_file_explorer_filter();
                        }
                        _ => {}
                    }
                } else {
                    match key_code {
                        crossterm::event::KeyCode::Esc => {
                            self.reset_file_explorer_filter();
                            self.input_mode = InputMode::Normal;
                        }
                        crossterm::event::KeyCode::Char('/') => {
                            self.file_explorer_search_active = true;
                        }
                        crossterm::event::KeyCode::Enter => {
                            let current = self.file_explorer.current().path.to_owned();
                            if current.is_file() {
                                self.prompt_input = current.to_string_lossy().to_string();
                                self.reset_file_explorer_filter();
                                self.start_upload().await;
                            } else {
                                let _ = self.file_explorer.handle(event);
                            }
                        }
                        crossterm::event::KeyCode::Char('F') => {
                            let current = self.file_explorer.current().path.to_owned();
                            if current.is_dir() {
                                self.prompt_input = current.to_string_lossy().to_string();
                                self.reset_file_explorer_filter();
                                self.start_folder_upload().await;
                            }
                        }
                        _ => {
                            let _ = self.file_explorer.handle(event);
                        }
                    }
                }
            }
            InputMode::DeleteConfirmation => match key_code {
                crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                    self.start_batch_delete().await;
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') | crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                _ => {}
            },
            InputMode::CreateFolderPrompt => match key_code {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Enter => {
                    if !self.prompt_input.trim().is_empty() {
                        self.create_folder().await;
                    }
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Backspace => {
                    if key_modifiers.contains(crossterm::event::KeyModifiers::SUPER) {
                        self.prompt_input.clear();
                    } else if key_modifiers.contains(crossterm::event::KeyModifiers::ALT) {
                        delete_previous_word(&mut self.prompt_input);
                    } else {
                        self.prompt_input.pop();
                    }
                }
                crossterm::event::KeyCode::Char('w') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    delete_previous_word(&mut self.prompt_input);
                }
                crossterm::event::KeyCode::Char('u') if key_modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                    self.prompt_input.clear();
                }
                crossterm::event::KeyCode::Char(c) => {
                    self.prompt_input.push(c);
                }
                _ => {}
            }
            InputMode::OverwriteConfirmation => match key_code {
                crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                    if let Some(state) = &mut self.conflict_state {
                        state.selected_option = (state.selected_option + 1) % 5;
                    }
                }
                crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                    if let Some(state) = &mut self.conflict_state {
                        state.selected_option = state.selected_option.checked_sub(1).unwrap_or(4);
                    }
                }
                crossterm::event::KeyCode::Enter => {
                    self.resolve_current_conflict().await;
                }
                crossterm::event::KeyCode::Esc => {
                    self.conflict_state = None;
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Char('o') => {
                    if let Some(state) = &mut self.conflict_state { state.selected_option = 0; }
                    self.resolve_current_conflict().await;
                }
                crossterm::event::KeyCode::Char('s') => {
                    if let Some(state) = &mut self.conflict_state { state.selected_option = 1; }
                    self.resolve_current_conflict().await;
                }
                crossterm::event::KeyCode::Char('r') => {
                    if let Some(state) = &mut self.conflict_state { state.selected_option = 2; }
                    self.resolve_current_conflict().await;
                }
                crossterm::event::KeyCode::Char('O') => {
                    if let Some(state) = &mut self.conflict_state { state.selected_option = 3; }
                    self.resolve_current_conflict().await;
                }
                crossterm::event::KeyCode::Char('S') => {
                    if let Some(state) = &mut self.conflict_state { state.selected_option = 4; }
                    self.resolve_current_conflict().await;
                }
                _ => {}
            }
        }
    }

    async fn create_folder(&mut self) {
        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let provider_idx = match self.active_provider_index {
            Some(idx) => idx,
            None => return,
        };

        let folder_name = self.prompt_input.trim().to_string();
        let folder_path = if let Some(p) = &self.current_path {
            let mut path_str = p.to_string();
            if !path_str.is_empty() && !path_str.ends_with('/') {
                path_str.push('/');
            }
            path_str.push_str(&folder_name);
            object_store::path::Path::from(path_str)
        } else {
            object_store::path::Path::from(folder_name.clone())
        };

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let id = format!("create_folder_{}_{}", bucket, folder_path);
        
        let task = AppTask {
            id: id.clone(),
            description: format!("Creating folder: {}", folder_name),
            task_type: TaskType::CreateFolder,
            bytes_transferred: 0,
            total_size: 0,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        tokio::spawn(async move {
            let result = provider.create_folder(&bucket, &folder_path).await;
            let _ = event_tx.send(AppEvent::TaskFinished {
                id,
                result: result.map_err(|e| e.to_string()),
            });
        });
    }

    async fn start_batch_delete(&mut self) {
        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let mut paths_to_delete = Vec::new();
        if !self.selected_paths.is_empty() {
            paths_to_delete.extend(self.selected_paths.iter().cloned());
        } else if let Some(index) = self.main_list_state.selected() {
            let filtered = self.filtered_objects();
            if let Some(obj) = filtered.get(index) {
                paths_to_delete.push(obj.path.clone());
            }
        }

        if paths_to_delete.is_empty() {
            return;
        }

        let sidebar_index = match self.sidebar_state.selected() {
            Some(i) => i,
            None => return,
        };
        let items = self.sidebar_items();
        let provider_idx = match items.get(sidebar_index) {
            Some(SidebarItem::Bucket { provider_index, .. }) => *provider_index,
            _ => return,
        };

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let item_count = paths_to_delete.len();
        let id = format!("delete_{}", std::time::Instant::now().elapsed().as_nanos());

        let scheme = self.storage_providers[provider_idx].provider_type().scheme();
        let description = if item_count == 1 {
            format!("Deleting {}://{}/{}", scheme, bucket, paths_to_delete[0])
        } else {
            format!("Deleting {} items", item_count)
        };

        let task = AppTask {
            id: id.clone(),
            description,
            task_type: TaskType::Delete,
            bytes_transferred: 0,
            total_size: 0,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        tokio::spawn(async move {
            let result = provider.delete_objects(&bucket, &paths_to_delete).await;
            let _ = event_tx.send(AppEvent::TaskFinished {
                id,
                result: result.map_err(|e| e.to_string()),
            });
        });
        self.selected_paths.clear();
    }

    fn on_enter(&mut self) {
        match self.active_pane {
            Pane::Sidebar => {
                if let Some(index) = self.sidebar_state.selected() {
                    let items = self.sidebar_items();
                    if let Some(item) = items.get(index) {
                        match item {
                            SidebarItem::Provider(p_idx) => {
                                let p_idx = *p_idx;
                                if self.expanded_providers.contains(&p_idx) {
                                    self.expanded_providers.remove(&p_idx);
                                } else if self.activated_providers.contains(&p_idx) {
                                    // Already connected — expand using cached buckets
                                    self.active_provider_index = Some(p_idx);
                                    self.expanded_providers.insert(p_idx);
                                    let items = self.sidebar_items();
                                    if let Some((item_idx, bucket_item)) = items.iter().enumerate().find(|(_, item)| {
                                        matches!(item, SidebarItem::Bucket { provider_index, .. } if *provider_index == p_idx)
                                    }) {
                                        if let SidebarItem::Bucket { provider_index, name, .. } = bucket_item {
                                            self.sidebar_state.select(Some(item_idx));
                                            self.current_bucket = Some(name.clone());
                                            self.current_path = None;
                                            self.search_query.clear();
                                            self.loading_message = Some("Loading...".to_string());
                                            self.pending_action = Some(PendingAction::LoadObjects {
                                                provider_index: *provider_index,
                                                bucket: name.clone(),
                                                path: None,
                                                set_current_path: false,
                                                highlight_path: None,
                                            });
                                        }
                                    }
                                } else {
                                    // First connection — schedule via pending action
                                    let provider_name = self.storage_providers
                                        .get(p_idx)
                                        .map(|p| p.name().to_string())
                                        .unwrap_or_default();
                                    self.loading_message = Some(format!("Connecting to '{}'...", provider_name));
                                    self.pending_action = Some(PendingAction::ExpandProvider {
                                        provider_index: p_idx,
                                        provider_name,
                                    });
                                }
                            }
                            SidebarItem::Bucket { provider_index, name, .. } => {
                                let provider_index = *provider_index;
                                let name = name.clone();
                                self.active_provider_index = Some(provider_index);
                                self.current_bucket = Some(name.clone());
                                self.current_path = None;
                                self.search_query.clear();
                                self.loading_message = Some("Loading...".to_string());
                                self.pending_action = Some(PendingAction::LoadObjects {
                                    provider_index,
                                    bucket: name,
                                    path: None,
                                    set_current_path: false,
                                    highlight_path: None,
                                });
                            }
                        }
                    }
                }
            }
            Pane::Main => {
                if let Some(index) = self.main_list_state.selected() {
                    let obj_info: Option<(ObjectPath, bool)> = {
                        let filtered = self.filtered_objects();
                        filtered.get(index).map(|o| (o.path.clone(), o.is_dir))
                    };
                    if let Some((obj_path, is_dir)) = obj_info {
                        if is_dir {
                            if let (Some(bucket_name), Some(sidebar_index)) =
                                (self.current_bucket.clone(), self.sidebar_state.selected())
                            {
                                let items = self.sidebar_items();
                                if let Some(SidebarItem::Bucket { provider_index, .. }) = items.get(sidebar_index) {
                                    self.loading_message = Some("Loading...".to_string());
                                    self.pending_action = Some(PendingAction::LoadObjects {
                                        provider_index: *provider_index,
                                        bucket: bucket_name,
                                        path: Some(obj_path),
                                        set_current_path: true,
                                        highlight_path: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn on_back(&mut self) {
        if let Some(bucket_name) = &self.current_bucket {
            if let Some(path) = &self.current_path {
                let to_highlight = path.clone();
                let parent = path.parent();
                if let Some(sidebar_index) = self.sidebar_state.selected() {
                    let items = self.sidebar_items();
                    if let Some(SidebarItem::Bucket { provider_index, .. }) = items.get(sidebar_index) {
                        self.loading_message = Some("Loading...".to_string());
                        self.pending_action = Some(PendingAction::LoadObjects {
                            provider_index: *provider_index,
                            bucket: bucket_name.clone(),
                            path: parent.map(|p| p.to_owned()),
                            set_current_path: true,
                            highlight_path: Some(to_highlight),
                        });
                    }
                }
            }
        }
    }

    pub fn start_pending_action(&mut self) {
        let action = match self.pending_action.take() {
            Some(a) => a,
            None => return,
        };

        match action {
            PendingAction::ExpandProvider { provider_index, provider_name } => {
                let provider = match self.storage_providers.get(provider_index) {
                    Some(p) => p.clone_box(),
                    None => {
                        self.loading_message = None;
                        return;
                    }
                };
                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let result = provider.list_buckets().await.map_err(|e| e.to_string());
                    let _ = tx.send(AppEvent::ConnectCompleted {
                        provider_index,
                        provider_name,
                        result,
                    });
                });
            }
            PendingAction::LoadObjects { provider_index, bucket, path, set_current_path, highlight_path } => {
                let provider = match self.storage_providers.get(provider_index) {
                    Some(p) => p.clone_box(),
                    None => {
                        self.loading_message = None;
                        return;
                    }
                };
                let tx = self.event_tx.clone();
                let path_for_call = path.clone();
                tokio::spawn(async move {
                    let result = provider
                        .list_objects(&bucket, path_for_call.as_ref())
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(AppEvent::ObjectsLoadCompleted {
                        path,
                        set_current_path,
                        highlight_path,
                        result,
                    });
                });
            }
        }
    }

    fn toggle_pane(&mut self) {
        match self.active_pane {
            Pane::Sidebar => self.active_pane = Pane::Main,
            Pane::Main => self.active_pane = Pane::Sidebar,
        }
    }

    async fn move_down(&mut self) {
        match self.active_pane {
            Pane::Sidebar => {
                let items = self.sidebar_items();
                let i = match self.sidebar_state.selected() {
                    Some(i) => {
                        if i >= items.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                if !items.is_empty() {
                    self.sidebar_state.select(Some(i));
                }
            }
            Pane::Main => {
                let filtered = self.filtered_objects();
                let i = match self.main_list_state.selected() {
                    Some(i) => {
                        if i >= filtered.len().saturating_sub(1) {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                if !filtered.is_empty() {
                    self.main_list_state.select(Some(i));
                    if self.preview_enabled {
                        self.update_preview().await;
                    }
                }
            }
        }
    }

    async fn move_up(&mut self) {
        match self.active_pane {
            Pane::Sidebar => {
                let items = self.sidebar_items();
                let i = match self.sidebar_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            items.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                if !items.is_empty() {
                    self.sidebar_state.select(Some(i));
                }
            }
            Pane::Main => {
                let filtered = self.filtered_objects();
                let i = match self.main_list_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            filtered.len().saturating_sub(1)
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                if !filtered.is_empty() {
                    self.main_list_state.select(Some(i));
                    if self.preview_enabled {
                        self.update_preview().await;
                    }
                }
            }
        }
    }

    pub async fn update_preview(&mut self) {
        self.preview_data = None;
        if self.active_pane == Pane::Main {
            if let Some(index) = self.main_list_state.selected() {
                let filtered = self.filtered_objects();
                if let Some(obj) = filtered.get(index) {
                    if !obj.is_dir {
                        if let Some(bucket) = &self.current_bucket {
                            if let Some(sidebar_index) = self.sidebar_state.selected() {
                                let items = self.sidebar_items();
                                if let Some(SidebarItem::Bucket { provider_index, .. }) = items.get(sidebar_index) {
                                    if let Some(provider) = self.storage_providers.get(*provider_index) {
                                        match provider.get_object_preview(bucket, &obj.path, 1024).await {
                                            Ok(bytes) => self.preview_data = Some(bytes),
                                            Err(e) => tracing::error!("Failed to get preview: {}", e),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn start_download(&mut self) {
        let local_base_path = std::path::PathBuf::from(&self.prompt_input);
        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let sidebar_index = match self.sidebar_state.selected() {
            Some(i) => i,
            None => return,
        };
        let items = self.sidebar_items();
        let provider_idx = match items.get(sidebar_index) {
            Some(SidebarItem::Bucket { provider_index, .. }) => *provider_index,
            _ => return,
        };

        // Collect selected items (files and folders)
        let mut selected_objects = Vec::new();
        if !self.selected_paths.is_empty() {
            for path in &self.selected_paths {
                let is_dir = self.objects.iter().any(|o| o.path == *path && o.is_dir);
                selected_objects.push((path.clone(), is_dir));
            }
        } else if let Some(index) = self.main_list_state.selected() {
            let filtered = self.filtered_objects();
            if let Some(obj) = filtered.get(index) {
                selected_objects.push((obj.path.clone(), obj.is_dir));
            }
        }

        // Flatten all items into download operations
        let provider = &self.storage_providers[provider_idx];
        let mut all_items: Vec<ConflictOperation> = Vec::new();

        for (path, is_dir) in &selected_objects {
            if *is_dir {
                let folder_name = path.filename().unwrap_or(path.as_ref()).to_string();
                match provider.list_objects_recursive(&bucket, Some(path)).await {
                    Ok(objects) => {
                        for obj in objects {
                            let rel = obj.path.as_ref().strip_prefix(path.as_ref())
                                .unwrap_or(obj.path.as_ref())
                                .trim_start_matches('/');
                            if rel.is_empty() { continue; }
                            let local_path = local_base_path.join(&folder_name).join(rel);
                            all_items.push(ConflictOperation::Download {
                                remote_path: obj.path,
                                local_path,
                                size: obj.size,
                            });
                        }
                    }
                    Err(e) => {
                        self.set_error(format!("Failed to list folder {}: {}", folder_name, e));
                        return;
                    }
                }
            } else {
                let filename = path.filename().unwrap_or(path.as_ref()).to_string();
                let local_path = local_base_path.join(&filename);
                let size = self.objects.iter().find(|o| o.path == *path).map(|o| o.size).unwrap_or(0);
                all_items.push(ConflictOperation::Download {
                    remote_path: path.clone(),
                    local_path,
                    size,
                });
            }
        }

        self.selected_paths.clear();

        if all_items.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        // Partition into conflicts (local file exists) and no-conflict
        let mut conflicts = Vec::new();
        let mut no_conflict = Vec::new();
        for item in all_items {
            if let ConflictOperation::Download { ref local_path, ref remote_path, .. } = item {
                if local_path.exists() {
                    conflicts.push(PendingConflict {
                        source_name: remote_path.filename().unwrap_or(remote_path.as_ref()).to_string(),
                        target_display: local_path.to_string_lossy().to_string(),
                        operation: item,
                    });
                } else {
                    no_conflict.push(item);
                }
            }
        }

        if conflicts.is_empty() {
            self.execute_download_batch(no_conflict, &bucket, provider_idx).await;
            self.input_mode = InputMode::Normal;
        } else {
            self.conflict_state = Some(ConflictResolutionState {
                conflicts,
                current_index: 0,
                resolved: Vec::new(),
                no_conflict_items: no_conflict,
                selected_option: 0,
                bucket,
                provider_idx,
                existing_remote_keys: None,
            });
            self.input_mode = InputMode::OverwriteConfirmation;
        }
    }

    async fn start_upload(&mut self) {
        let local_path = std::path::PathBuf::from(&self.prompt_input);
        if !local_path.exists() {
            self.set_error("Local file does not exist".to_string());
            return;
        }

        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let filename = local_path.file_name().and_then(|n| n.to_str()).unwrap_or("uploaded_file").to_string();
        let remote_path = if let Some(p) = &self.current_path {
            p.child(&*filename)
        } else {
            object_store::path::Path::from(&*filename)
        };

        let sidebar_index = match self.sidebar_state.selected() {
            Some(i) => i,
            None => return,
        };
        let items = self.sidebar_items();
        let provider_idx = match items.get(sidebar_index) {
            Some(SidebarItem::Bucket { provider_index, .. }) => *provider_index,
            _ => return,
        };

        let total_size = std::fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
        let key_exists = self.objects.iter().any(|o| o.path == remote_path && !o.is_dir);

        let item = ConflictOperation::Upload {
            local_path,
            remote_path: remote_path.clone(),
            size: total_size,
        };

        if key_exists {
            let existing_keys: HashSet<String> = self.objects.iter()
                .filter(|o| !o.is_dir)
                .map(|o| o.path.to_string())
                .collect();
            self.conflict_state = Some(ConflictResolutionState {
                conflicts: vec![PendingConflict {
                    source_name: filename,
                    target_display: remote_path.to_string(),
                    operation: item,
                }],
                current_index: 0,
                resolved: Vec::new(),
                no_conflict_items: Vec::new(),
                selected_option: 0,
                bucket,
                provider_idx,
                existing_remote_keys: Some(existing_keys),
            });
            self.input_mode = InputMode::OverwriteConfirmation;
        } else {
            self.execute_upload_batch(vec![item], &bucket, provider_idx).await;
            self.input_mode = InputMode::Normal;
        }
    }

    async fn start_folder_upload(&mut self) {
        let folder_path = std::path::PathBuf::from(&self.prompt_input);
        if !folder_path.is_dir() {
            self.set_error("Selected path is not a directory".to_string());
            return;
        }

        let bucket = match &self.current_bucket {
            Some(b) => b.clone(),
            None => return,
        };

        let sidebar_index = match self.sidebar_state.selected() {
            Some(i) => i,
            None => return,
        };
        let items = self.sidebar_items();
        let provider_idx = match items.get(sidebar_index) {
            Some(SidebarItem::Bucket { provider_index, .. }) => *provider_index,
            _ => return,
        };

        let folder_name = folder_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("folder")
            .to_string();

        // Collect all files recursively
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let mut dirs_to_visit = vec![folder_path.clone()];
        while let Some(dir) = dirs_to_visit.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(err) => {
                    self.set_error(format!("Failed to read directory {}: {}", dir.display(), err));
                    return;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs_to_visit.push(path);
                } else if path.is_file() {
                    files.push(path);
                }
            }
        }

        if files.is_empty() {
            self.set_error("Selected folder is empty".to_string());
            return;
        }

        let base_remote = if let Some(p) = &self.current_path {
            p.child(&*folder_name)
        } else {
            ObjectPath::from(&*folder_name)
        };

        // Build all upload operations with their remote paths
        let mut all_items: Vec<ConflictOperation> = Vec::new();
        for file_path in &files {
            let relative = file_path.strip_prefix(&folder_path).unwrap_or(file_path);
            let mut remote_path = base_remote.clone();
            for component in relative.parent().into_iter().flat_map(|p| p.components()) {
                if let std::path::Component::Normal(seg) = component {
                    if let Some(s) = seg.to_str() {
                        remote_path = remote_path.child(s);
                    }
                }
            }
            let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            remote_path = remote_path.child(file_name);
            let size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);
            all_items.push(ConflictOperation::Upload {
                local_path: file_path.clone(),
                remote_path,
                size,
            });
        }

        // Check for remote conflicts by listing existing objects under the target prefix
        let provider = &self.storage_providers[provider_idx];
        let existing_keys: HashSet<String> = match provider.list_objects_recursive(&bucket, Some(&base_remote)).await {
            Ok(objects) => objects.into_iter().filter(|o| !o.is_dir).map(|o| o.path.to_string()).collect(),
            Err(_) => HashSet::new(),
        };

        let mut conflicts = Vec::new();
        let mut no_conflict = Vec::new();
        for item in all_items {
            if let ConflictOperation::Upload { ref remote_path, ref local_path, .. } = item {
                if existing_keys.contains(&remote_path.to_string()) {
                    conflicts.push(PendingConflict {
                        source_name: local_path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string(),
                        target_display: remote_path.to_string(),
                        operation: item,
                    });
                } else {
                    no_conflict.push(item);
                }
            }
        }

        if conflicts.is_empty() {
            self.execute_upload_batch(no_conflict, &bucket, provider_idx).await;
            self.input_mode = InputMode::Normal;
        } else {
            self.conflict_state = Some(ConflictResolutionState {
                conflicts,
                current_index: 0,
                resolved: Vec::new(),
                no_conflict_items: no_conflict,
                selected_option: 0,
                bucket,
                provider_idx,
                existing_remote_keys: Some(existing_keys),
            });
            self.input_mode = InputMode::OverwriteConfirmation;
        }
    }

    async fn execute_download_batch(&mut self, items: Vec<ConflictOperation>, bucket: &str, provider_idx: usize) {
        if items.is_empty() { return; }

        let total_size: u64 = items.iter().map(|item| {
            if let ConflictOperation::Download { size, .. } = item { *size } else { 0 }
        }).sum();

        let scheme = self.storage_providers[provider_idx].provider_type().scheme();
        let description = if items.len() == 1 {
            if let ConflictOperation::Download { ref remote_path, ref local_path, .. } = items[0] {
                format!("Downloading {}://{}/{} to {}", scheme, bucket, remote_path, local_path.display())
            } else {
                "Downloading".to_string()
            }
        } else {
            format!("Downloading {} items", items.len())
        };

        let id = format!("download_batch_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        let task = AppTask {
            id: id.clone(),
            description,
            task_type: TaskType::Download,
            bytes_transferred: 0,
            total_size,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        let file_count = items.len();
        let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(file_count));
        let first_error: Arc<tokio::sync::Mutex<Option<String>>> = Arc::new(tokio::sync::Mutex::new(None));

        for item in items {
            if let ConflictOperation::Download { remote_path, local_path, .. } = item {
                let provider = self.storage_providers[provider_idx].clone_box();
                let event_tx = self.event_tx.clone();
                let task_id = id.clone();
                let bucket = bucket.to_string();
                let remaining = remaining.clone();
                let first_error = first_error.clone();

                tokio::spawn(async move {
                    let event_tx_inner = event_tx.clone();
                    let task_id_inner = task_id.clone();
                    let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                        let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
                    });

                    let result = provider.download(&bucket, &remote_path, &local_path, progress_cb).await;
                    if let Err(e) = &result {
                        let mut err = first_error.lock().await;
                        if err.is_none() {
                            *err = Some(e.to_string());
                        }
                    }

                    if remaining.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                        let err = first_error.lock().await;
                        let result = match &*err {
                            Some(e) => Err(e.clone()),
                            None => Ok(()),
                        };
                        let _ = event_tx.send(AppEvent::TaskFinished {
                            id: task_id,
                            result,
                        });
                    }
                });
            }
        }
    }

    async fn execute_upload_batch(&mut self, items: Vec<ConflictOperation>, bucket: &str, provider_idx: usize) {
        if items.is_empty() { return; }

        let total_size: u64 = items.iter().map(|item| {
            if let ConflictOperation::Upload { size, .. } = item { *size } else { 0 }
        }).sum();

        let scheme = self.storage_providers[provider_idx].provider_type().scheme();
        let description = if items.len() == 1 {
            if let ConflictOperation::Upload { ref local_path, ref remote_path, .. } = items[0] {
                format!("Uploading {} to {}://{}/{}", local_path.display(), scheme, bucket, remote_path)
            } else {
                "Uploading".to_string()
            }
        } else {
            format!("Uploading {} items", items.len())
        };

        let id = format!("upload_batch_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        let task = AppTask {
            id: id.clone(),
            description,
            task_type: TaskType::Upload,
            bytes_transferred: 0,
            total_size,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let task_id = id.clone();
        let bucket = bucket.to_string();

        tokio::spawn(async move {
            let mut overall_result: Result<(), String> = Ok(());

            for item in &items {
                if let ConflictOperation::Upload { local_path, remote_path, .. } = item {
                    let event_tx_inner = event_tx.clone();
                    let task_id_inner = task_id.clone();
                    let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                        let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
                    });

                    if let Err(e) = provider.upload(&bucket, remote_path, local_path, progress_cb).await {
                        overall_result = Err(format!("{}: {}", local_path.display(), e));
                        break;
                    }
                }
            }

            let _ = event_tx.send(AppEvent::TaskFinished {
                id: task_id,
                result: overall_result,
            });
        });
    }

    async fn resolve_current_conflict(&mut self) {
        let choice = {
            let state = match &self.conflict_state {
                Some(s) => s,
                None => return,
            };
            match state.selected_option {
                0 => ConflictChoice::Overwrite,
                1 => ConflictChoice::Skip,
                2 => ConflictChoice::Rename,
                3 => ConflictChoice::OverwriteAll,
                4 => ConflictChoice::SkipAll,
                _ => return,
            }
        };

        let state = self.conflict_state.as_mut().unwrap();

        if choice == ConflictChoice::OverwriteAll || choice == ConflictChoice::SkipAll {
            let effective = if choice == ConflictChoice::OverwriteAll {
                ConflictChoice::Overwrite
            } else {
                ConflictChoice::Skip
            };
            while state.current_index < state.conflicts.len() {
                let conflict = state.conflicts[state.current_index].clone();
                state.resolved.push((conflict, effective.clone()));
                state.current_index += 1;
            }
        } else {
            let conflict = state.conflicts[state.current_index].clone();
            state.resolved.push((conflict, choice));
            state.current_index += 1;
        }

        if state.current_index >= state.conflicts.len() {
            self.finalize_conflict_resolution().await;
        } else {
            let state = self.conflict_state.as_mut().unwrap();
            state.selected_option = 0;
        }
    }

    async fn finalize_conflict_resolution(&mut self) {
        let state = match self.conflict_state.take() {
            Some(s) => s,
            None => return,
        };

        self.input_mode = InputMode::Normal;

        let mut items_to_execute: Vec<ConflictOperation> = state.no_conflict_items;

        for (conflict, choice) in state.resolved {
            match choice {
                ConflictChoice::Skip => {}
                ConflictChoice::Overwrite => {
                    items_to_execute.push(conflict.operation);
                }
                ConflictChoice::Rename => {
                    match conflict.operation {
                        ConflictOperation::Download { remote_path, local_path, size } => {
                            let renamed = auto_rename_local_path(&local_path);
                            items_to_execute.push(ConflictOperation::Download {
                                remote_path,
                                local_path: renamed,
                                size,
                            });
                        }
                        ConflictOperation::Upload { local_path, remote_path, size } => {
                            let existing = state.existing_remote_keys.as_ref()
                                .cloned()
                                .unwrap_or_default();
                            let renamed = auto_rename_remote_path(&remote_path, &existing);
                            items_to_execute.push(ConflictOperation::Upload {
                                local_path,
                                remote_path: renamed,
                                size,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        if items_to_execute.is_empty() {
            return;
        }

        // Separate downloads and uploads (a single operation only produces one type)
        let downloads: Vec<ConflictOperation> = items_to_execute.iter()
            .filter(|i| matches!(i, ConflictOperation::Download { .. }))
            .cloned()
            .collect();
        let uploads: Vec<ConflictOperation> = items_to_execute.into_iter()
            .filter(|i| matches!(i, ConflictOperation::Upload { .. }))
            .collect();

        if !downloads.is_empty() {
            self.execute_download_batch(downloads, &state.bucket, state.provider_idx).await;
        }
        if !uploads.is_empty() {
            self.execute_upload_batch(uploads, &state.bucket, state.provider_idx).await;
        }
    }
}

pub fn delete_previous_word(s: &mut String) {
    // Skip trailing whitespace
    while s.ends_with(|c: char| c.is_whitespace()) {
        s.pop();
    }
    // Delete until next whitespace or path separator
    while !s.is_empty() && !s.ends_with(|c: char| c.is_whitespace() || c == '/' || c == '\\') {
        s.pop();
    }
}

fn get_editing_field_mut(state: &mut ConfigModalState) -> Option<&mut String> {
    let idx = state.selected_field_index;
    let provider = state.providers.get_mut(state.selected_provider_index)?;
    provider.get_field_mut(idx)
}

fn auto_rename_local_path(path: &std::path::Path) -> std::path::PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let mut counter = 1u32;
    loop {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, counter, e),
            None => format!("{} ({})", stem, counter),
        };
        let candidate = parent.join(&new_name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn auto_rename_remote_path(path: &ObjectPath, existing: &HashSet<String>) -> ObjectPath {
    let path_str = path.to_string();
    let (dir, file) = if let Some(pos) = path_str.rfind('/') {
        (&path_str[..=pos], &path_str[pos + 1..])
    } else {
        ("", path_str.as_str())
    };
    let (stem, ext) = if let Some(dot_pos) = file.rfind('.') {
        (&file[..dot_pos], Some(&file[dot_pos..]))
    } else {
        (file, None)
    };
    let mut counter = 1u32;
    loop {
        let new_key = match ext {
            Some(e) => format!("{}{} ({}){}", dir, stem, counter, e),
            None => format!("{}{} ({})", dir, stem, counter),
        };
        if !existing.contains(&new_key) {
            return ObjectPath::from(new_key);
        }
        counter += 1;
    }
}
