use crate::config::{Config, ProviderConfig, ProviderType};
use crate::storage::{Bucket, Object, StorageBackend, ProgressCallback};
use object_store::path::Path as ObjectPath;
use crate::storage::s3::S3Storage;
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
}

#[derive(PartialEq, Eq)]
pub enum Pane {
    Sidebar,
    Main,
}

pub struct ConfigModalState {
    pub providers: Vec<ProviderConfig>,
    pub selected_provider_index: usize,
    pub selected_field_index: usize,
    pub edit_mode: bool, // true if editing a field, false if navigating providers
}

#[derive(Clone)]
pub enum SidebarItem {
    Provider(usize),
    Bucket { provider_index: usize, name: String },
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

pub enum AppEvent {
    TaskProgress { id: String, bytes: u64 },
    TaskFinished { id: String, result: Result<(), String> },
    RequestRefresh,
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
    pub objects: Vec<Object>,
    pub selected_paths: HashSet<object_store::path::Path>,
    pub current_bucket: Option<String>,
    pub current_path: Option<object_store::path::Path>,
    pub preview_data: Option<bytes::Bytes>,
    pub search_query: String,

    // Task State
    pub active_tasks: Vec<AppTask>,
    pub last_finished_task: Option<FinishedTask>,
    pub session_tasks: Vec<FinishedTask>,
    pub prompt_input: String,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,

    // File Explorer for Upload
    pub file_explorer: FileExplorer,

    pub copy_button_area: Option<ratatui::layout::Rect>,
    pub error_copy_area: Option<ratatui::layout::Rect>,
    pub task_copy_area: Option<ratatui::layout::Rect>,
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
            objects: Vec::new(),
            selected_paths: HashSet::new(),
            current_bucket: None,
            current_path: None,
            preview_data: None,
            search_query: String::new(),
            active_tasks: Vec::new(),
            last_finished_task: None,
            session_tasks: Vec::new(),
            prompt_input: String::new(),
            event_tx,
            event_rx,
            file_explorer,
            copy_button_area: None,
            error_copy_area: None,
            task_copy_area: None,
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
                        });
                    }
                }
            }
        }
        items
    }

    pub async fn refresh_storage_providers(&mut self) {
        let mut storage_providers: Vec<Box<dyn StorageBackend>> = Vec::new();
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
                _ => {}
            }
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
                self.handle_key(key.code, event).await;
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

    async fn handle_key(&mut self, key_code: crossterm::event::KeyCode, event: &crossterm::event::Event) {
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
                crossterm::event::KeyCode::Char('c') => {
                    self.show_config_modal = !self.show_config_modal;
                    if self.show_config_modal {
                        self.config_modal_state = Some(ConfigModalState {
                            providers: self.config.providers.clone(),
                            selected_provider_index: 0,
                            selected_field_index: 0,
                            edit_mode: false,
                        });
                        if self.config_modal_state.as_ref().unwrap().providers.is_empty() {
                            self.config_modal_state.as_mut().unwrap().providers.push(ProviderConfig::default());
                        }
                    } else {
                        self.config_modal_state = None;
                    }
                }
                crossterm::event::KeyCode::Char('D') if self.show_config_modal => {
                    if let Some(state) = &self.config_modal_state {
                        if !state.providers.is_empty() {
                             self.pending_delete_index = Some(state.selected_provider_index);
                        }
                    }
                }
                crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            if state.selected_provider_index < state.providers.len().saturating_sub(1) {
                                state.selected_provider_index += 1;
                            }
                        }
                    } else {
                        self.move_down().await;
                    }
                }
                crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            if state.selected_provider_index > 0 {
                                state.selected_provider_index -= 1;
                            }
                        }
                    } else {
                        self.move_up().await;
                    }
                }
                crossterm::event::KeyCode::Char('a') if self.show_config_modal => {
                    if let Some(state) = &mut self.config_modal_state {
                        state.providers.push(ProviderConfig::default());
                        state.selected_provider_index = state.providers.len() - 1;
                    }
                }
                crossterm::event::KeyCode::Char('e') if self.show_config_modal => {
                    if let Some(state) = &mut self.config_modal_state {
                        if !state.providers.is_empty() {
                            self.input_mode = InputMode::Editing;
                            state.edit_mode = true;
                        }
                    }
                }
                crossterm::event::KeyCode::Char('s') if self.show_config_modal => {
                    if let Some(state) = &self.config_modal_state {
                        self.config.providers = state.providers.clone();
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
                                let config_path = dirs::home_dir().unwrap().join(".config").join("stuie").join("config.json");
                                self.save_message = Some(format!("Configuration saved to {}!", config_path.display()));
                                self.save_message_time = Some(std::time::Instant::now());
                            }
                            Err(e) => self.set_error(format!("Failed to save config: {}", e)),
                        }
                    }
                }
                crossterm::event::KeyCode::Tab | crossterm::event::KeyCode::BackTab => {
                    self.toggle_pane();
                }
                crossterm::event::KeyCode::Enter => {
                    if self.show_config_modal {
                        if let Some(state) = &mut self.config_modal_state {
                            if let Some(provider) = state.providers.get_mut(state.selected_provider_index) {
                                provider.enabled = !provider.enabled;
                            }
                        }
                    } else {
                        self.on_enter().await;
                    }
                }
                crossterm::event::KeyCode::Backspace => {
                    if !self.show_config_modal {
                        self.on_back().await;
                    }
                }
                _ => {}
            },
            InputMode::Editing => match key_code {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                    if let Some(state) = &mut self.config_modal_state {
                        state.edit_mode = false;
                    }
                }
                crossterm::event::KeyCode::Tab => {
                    if let Some(state) = &mut self.config_modal_state {
                        state.selected_field_index = (state.selected_field_index + 1) % 8;
                    }
                }
                crossterm::event::KeyCode::BackTab => {
                    if let Some(state) = &mut self.config_modal_state {
                        if state.selected_field_index == 0 {
                            state.selected_field_index = 7;
                        } else {
                            state.selected_field_index -= 1;
                        }
                    }
                }
                crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                    if let Some(state) = &mut self.config_modal_state {
                        let provider = &mut state.providers[state.selected_provider_index];
                        if state.selected_field_index == 1 {
                             provider.provider_type = match provider.provider_type {
                                 ProviderType::S3 => ProviderType::GCS,
                                 ProviderType::GCS => ProviderType::Azure,
                                 ProviderType::Azure => ProviderType::S3,
                             };
                        } else if state.selected_field_index == 7 {
                             provider.allow_http = Some(!provider.allow_http.unwrap_or(false));
                        }
                    }
                }
                crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                    if let Some(state) = &mut self.config_modal_state {
                        let provider = &mut state.providers[state.selected_provider_index];
                        if state.selected_field_index == 1 {
                             provider.provider_type = match provider.provider_type {
                                 ProviderType::S3 => ProviderType::Azure,
                                 ProviderType::GCS => ProviderType::S3,
                                 ProviderType::Azure => ProviderType::GCS,
                             };
                        } else if state.selected_field_index == 7 {
                             provider.allow_http = Some(!provider.allow_http.unwrap_or(false));
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
                        let provider = &mut state.providers[state.selected_provider_index];
                        match state.selected_field_index {
                            0 => { provider.name.pop(); }
                            2 => { if let Some(r) = &mut provider.region { r.pop(); } }
                            3 => { if let Some(b) = &mut provider.bucket_name { b.pop(); } }
                            4 => { if let Some(e) = &mut provider.endpoint { e.pop(); } }
                            5 => { if let Some(a) = &mut provider.access_key_id { a.pop(); } }
                            6 => { if let Some(s) = &mut provider.secret_access_key { s.pop(); } }
                            _ => {}
                        }
                    }
                }
                crossterm::event::KeyCode::Char(c) => {
                    if let Some(state) = &mut self.config_modal_state {
                        let provider = &mut state.providers[state.selected_provider_index];
                        match state.selected_field_index {
                            0 => { provider.name.push(c); }
                            1 | 7 => {
                                // Ignore char input for Type and HTTP, only cycle with arrows/j/k
                            }
                            2 => { provider.region.get_or_insert_with(String::new).push(c); }
                            3 => { provider.bucket_name.get_or_insert_with(String::new).push(c); }
                            4 => { provider.endpoint.get_or_insert_with(String::new).push(c); }
                            5 => { provider.access_key_id.get_or_insert_with(String::new).push(c); }
                            6 => { provider.secret_access_key.get_or_insert_with(String::new).push(c); }
                            _ => {}
                        }
                    }
                }
                _ => {}
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
                    self.search_query.pop();
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
            InputMode::DownloadPrompt => match key_code {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Enter => {
                    let current = self.file_explorer.current().path.to_owned();
                    if current.is_dir() {
                        self.prompt_input = current.to_string_lossy().to_string();
                        self.start_download().await;
                        self.input_mode = InputMode::Normal;
                    } else {
                        // If a file is selected, use its parent directory
                        if let Some(parent) = current.parent() {
                            self.prompt_input = parent.to_string_lossy().to_string();
                            self.start_download().await;
                            self.input_mode = InputMode::Normal;
                        } else {
                            let _ = self.file_explorer.handle(event);
                        }
                    }
                }
                _ => {
                    let _ = self.file_explorer.handle(event);
                }
            }
            InputMode::UploadPrompt => match key_code {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Enter => {
                    let current = self.file_explorer.current().path.to_owned();
                    if current.is_file() {
                        self.prompt_input = current.to_string_lossy().to_string();
                        self.start_upload().await;
                        self.input_mode = InputMode::Normal;
                    } else {
                        let _ = self.file_explorer.handle(event);
                    }
                }
                crossterm::event::KeyCode::Char('F') => {
                    let current = self.file_explorer.current().path.to_owned();
                    if current.is_dir() {
                        self.prompt_input = current.to_string_lossy().to_string();
                        self.start_folder_upload().await;
                        self.input_mode = InputMode::Normal;
                    }
                }
                _ => {
                    let _ = self.file_explorer.handle(event);
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
                    self.prompt_input.pop();
                }
                crossterm::event::KeyCode::Char(c) => {
                    self.prompt_input.push(c);
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
        
        let task = AppTask {
            id: id.clone(),
            description: format!("Deleting {} items", item_count),
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

    async fn on_enter(&mut self) {
        match self.active_pane {
            Pane::Sidebar => {
                if let Some(index) = self.sidebar_state.selected() {
                    let items = self.sidebar_items();
                    if let Some(item) = items.get(index) {
                        match item {
                            SidebarItem::Provider(p_idx) => {
                                if self.expanded_providers.contains(p_idx) {
                                    self.expanded_providers.remove(p_idx);
                                } else {
                                    if let Some(provider) = self.storage_providers.get(*p_idx) {
                                        let provider_name = provider.name().to_string();
                                        match provider.list_buckets().await {
                                            Ok(_) => {
                                                self.refresh_storage_providers().await;
                                                self.active_provider_index = Some(*p_idx);
                                                self.expanded_providers.insert(*p_idx);
                                                let items = self.sidebar_items();
                                                if let Some((item_idx, bucket_item)) = items.iter().enumerate().find(|(_, item)| {
                                                    matches!(item, SidebarItem::Bucket { provider_index, .. } if *provider_index == *p_idx)
                                                }) {
                                                    // Auto-select the first bucket
                                                    if let SidebarItem::Bucket { provider_index, name } = bucket_item {
                                                        self.sidebar_state.select(Some(item_idx));
                                                        self.current_bucket = Some(name.clone());
                                                        self.current_path = None;
                                                        self.search_query.clear();
                                                        if let Some(provider) = self.storage_providers.get(*provider_index) {
                                                            match provider.list_objects(name, None).await {
                                                                Ok(objects) => {
                                                                    self.objects = objects;
                                                                    if !self.objects.is_empty() {
                                                                        self.main_list_state.select(Some(0));
                                                                    } else {
                                                                        self.main_list_state.select(None);
                                                                    }
                                                                    self.update_preview().await;
                                                                }
                                                                Err(e) => self.set_error(format!("Failed to list objects in {}: {}", name, e)),
                                                            }
                                                        }
                                                    }
                                                    self.save_message = Some(format!("Connected to '{}'", provider_name));
                                                    self.save_message_time = Some(std::time::Instant::now());
                                                } else {
                                                    self.save_message = Some(format!("Connected to '{}', but no buckets found.", provider_name));
                                                    self.save_message_time = Some(std::time::Instant::now());
                                                }
                                            }
                                            Err(e) => self.set_error(format!("Connection to '{}' failed: {}", provider_name, e)),
                                        }
                                    }
                                }
                            }
                            SidebarItem::Bucket { provider_index, name } => {
                                if let Some(provider) = self.storage_providers.get(*provider_index) {
                                    self.active_provider_index = Some(*provider_index);
                                    self.current_bucket = Some(name.clone());
                                    self.current_path = None;
                                    self.search_query.clear();
                                    match provider.list_objects(name, None).await {
                                        Ok(objects) => {
                                            self.objects = objects;
                                            if !self.objects.is_empty() {
                                                self.main_list_state.select(Some(0));
                                            } else {
                                                self.main_list_state.select(None);
                                            }
                                            self.update_preview().await;
                                        }
                                        Err(e) => self.set_error(format!("Failed to list objects in {}: {}", name, e)),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Pane::Main => {
                // Handle entering folders
                if let Some(index) = self.main_list_state.selected() {
                    let filtered = self.filtered_objects();
                    if let Some(obj) = filtered.get(index).cloned() {
                        if obj.is_dir {
                            if let Some(bucket_name) = &self.current_bucket {
                                if let Some(sidebar_index) = self.sidebar_state.selected() {
                                    let items = self.sidebar_items();
                                    if let Some(SidebarItem::Bucket { provider_index, .. }) = items.get(sidebar_index) {
                                        if let Some(provider) = self.storage_providers.get(*provider_index) {
                                            match provider.list_objects(bucket_name, Some(&obj.path)).await {
                                                Ok(objects) => {
                                                    self.current_path = Some(obj.path.clone());
                                                    self.objects = objects;
                                                    self.search_query.clear();
                                                    if !self.objects.is_empty() {
                                                        self.main_list_state.select(Some(0));
                                                    } else {
                                                        self.main_list_state.select(None);
                                                    }
                                                    self.update_preview().await;
                                                }
                                                Err(e) => self.set_error(format!("Failed to list objects in {}: {}", obj.path, e)),
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
    }

    async fn on_back(&mut self) {
        if let Some(bucket_name) = &self.current_bucket {
            if let Some(path) = &self.current_path {
                let to_highlight = path.clone();
                let parent = path.parent();
                if let Some(sidebar_index) = self.sidebar_state.selected() {
                    let items = self.sidebar_items();
                    if let Some(SidebarItem::Bucket { provider_index, .. }) = items.get(sidebar_index) {
                        if let Some(provider) = self.storage_providers.get(*provider_index) {
                            match provider.list_objects(bucket_name, parent.as_ref()).await {
                                Ok(objects) => {
                                    self.current_path = parent.map(|p| p.to_owned());
                                    self.objects = objects;
                                    self.search_query.clear();
                                    
                                    // Try to find the previous folder in the new list
                                    let mut new_index = 0;
                                    if let Some(idx) = self.objects.iter().position(|obj| obj.path == to_highlight) {
                                        new_index = idx;
                                    }

                                    if !self.objects.is_empty() {
                                        self.main_list_state.select(Some(new_index));
                                    } else {
                                        self.main_list_state.select(None);
                                    }
                                    self.update_preview().await;
                                }
                                Err(e) => self.set_error(format!("Failed to list objects: {}", e)),
                            }
                        }
                    }
                }
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
                    self.update_preview().await;
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
                    self.update_preview().await;
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

        let provider = &self.storage_providers[provider_idx];
        for (path, is_dir) in &selected_objects {
            if *is_dir {
                // Folder download: single task for the entire folder
                let folder_name = path.filename().unwrap_or(path.as_ref()).to_string();
                let files: Vec<(ObjectPath, std::path::PathBuf, u64)> = match provider.list_objects_recursive(&bucket, Some(path)).await {
                    Ok(objects) => {
                        objects.into_iter().filter_map(|obj| {
                            let rel = obj.path.as_ref().strip_prefix(path.as_ref())
                                .unwrap_or(obj.path.as_ref())
                                .trim_start_matches('/');
                            if rel.is_empty() {
                                return None;
                            }
                            let local_path = local_base_path.join(&folder_name).join(rel);
                            Some((obj.path, local_path, obj.size))
                        }).collect()
                    }
                    Err(e) => {
                        self.set_error(format!("Failed to list folder {}: {}", folder_name, e));
                        return;
                    }
                };

                if files.is_empty() {
                    continue;
                }

                let total_size: u64 = files.iter().map(|(_, _, size)| size).sum();
                let id = format!("download_{}_{}", bucket, path);
                let task = AppTask {
                    id: id.clone(),
                    description: format!("Downloading {}/ to {}", folder_name, local_base_path.display()),
                    task_type: TaskType::Download,
                    bytes_transferred: 0,
                    total_size,
                    start_time: std::time::Instant::now(),
                };
                self.active_tasks.push(task);

                let file_count = files.len();
                let remaining = Arc::new(std::sync::atomic::AtomicUsize::new(file_count));
                let first_error: Arc<tokio::sync::Mutex<Option<String>>> = Arc::new(tokio::sync::Mutex::new(None));

                for (obj_path, local_path, _) in files {
                    let provider = self.storage_providers[provider_idx].clone_box();
                    let event_tx = self.event_tx.clone();
                    let task_id = id.clone();
                    let bucket_inner = bucket.clone();
                    let remaining = remaining.clone();
                    let first_error = first_error.clone();

                    tokio::spawn(async move {
                        let event_tx_inner = event_tx.clone();
                        let task_id_inner = task_id.clone();
                        let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                            let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
                        });

                        let result = provider.download(&bucket_inner, &obj_path, &local_path, progress_cb).await;
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
            } else {
                // Single file download
                let filename = path.filename().unwrap_or(path.as_ref()).to_string();
                let local_path = local_base_path.join(&filename);
                let bucket_inner = bucket.clone();
                let id = format!("download_{}_{}", bucket_inner, path);
                let total_size = self.objects.iter().find(|o| o.path == *path).map(|o| o.size).unwrap_or(0);

                let task = AppTask {
                    id: id.clone(),
                    description: format!("Downloading {} to {}", filename, local_base_path.join(&filename).display()),
                    task_type: TaskType::Download,
                    bytes_transferred: 0,
                    total_size,
                    start_time: std::time::Instant::now(),
                };
                self.active_tasks.push(task);

                let provider = self.storage_providers[provider_idx].clone_box();
                let event_tx = self.event_tx.clone();
                let task_id = id.clone();
                let path = path.clone();

                tokio::spawn(async move {
                    let event_tx_inner = event_tx.clone();
                    let task_id_inner = task_id.clone();
                    let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                        let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
                    });

                    let result = provider.download(&bucket_inner, &path, &local_path, progress_cb).await;
                    let _ = event_tx.send(AppEvent::TaskFinished {
                        id: task_id,
                        result: result.map_err(|e| e.to_string())
                    });
                });
            }
        }
        self.selected_paths.clear();
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
        let id = format!("upload_{}_{}", bucket, remote_path);

        let task = AppTask {
            id: id.clone(),
            description: format!("Uploading {} to {}", filename, self.get_full_remote_path()),
            task_type: TaskType::Upload,
            bytes_transferred: 0,
            total_size,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let event_tx_inner = event_tx.clone();
            let task_id_inner = task_id.clone();
            let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
            });

            let result = provider.upload(&bucket, &remote_path, &local_path, progress_cb).await;
            let _ = event_tx.send(AppEvent::TaskFinished {
                id: task_id,
                result: result.map_err(|e| e.to_string())
            });
        });
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

        let total_size: u64 = files.iter()
            .filter_map(|f| std::fs::metadata(f).ok())
            .map(|m| m.len())
            .sum();

        let base_remote = if let Some(p) = &self.current_path {
            p.child(&*folder_name)
        } else {
            ObjectPath::from(&*folder_name)
        };

        let id = format!("folder_upload_{}_{}", bucket, base_remote);
        let task = AppTask {
            id: id.clone(),
            description: format!("Uploading folder {} ({} files) to {}", folder_name, files.len(), self.get_full_remote_path()),
            task_type: TaskType::Upload,
            bytes_transferred: 0,
            total_size,
            start_time: std::time::Instant::now(),
        };
        self.active_tasks.push(task);

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let mut overall_result: Result<(), String> = Ok(());

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
                let file_name = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file");
                remote_path = remote_path.child(file_name);

                let event_tx_inner = event_tx.clone();
                let task_id_inner = task_id.clone();
                let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                    let _ = event_tx_inner.send(AppEvent::TaskProgress { id: task_id_inner.clone(), bytes });
                });

                if let Err(e) = provider.upload(&bucket, &remote_path, file_path, progress_cb).await {
                    overall_result = Err(format!("{}: {}", file_path.display(), e));
                    break;
                }
            }

            let _ = event_tx.send(AppEvent::TaskFinished {
                id: task_id,
                result: overall_result,
            });
        });
    }
}
