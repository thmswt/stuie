use crate::config::{Config, ProviderConfig, ProviderType};
use crate::storage::{Bucket, Object, StorageBackend, ProgressCallback};
use crate::storage::s3::S3Storage;
use ratatui::widgets::{ListState, TableState};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;

pub enum InputMode {
    Normal,
    Editing,
    Search,
    DownloadPrompt,
    UploadPrompt,
    DeleteConfirmation,
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

pub struct TransferTask {
    pub id: String,
    pub filename: String,
    pub is_upload: bool,
    pub bytes_transferred: u64,
    pub total_size: u64,
    pub start_time: std::time::Instant,
    pub target_path: String,
}

#[derive(Clone)]
pub struct FinishedTransfer {
    pub filename: String,
    pub target_path: String,
    pub is_upload: bool,
    pub result: Result<(), String>,
}

pub enum AppEvent {
    TransferProgress { id: String, bytes: u64 },
    TransferFinished { id: String, result: Result<(), String> },
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

    // Transfer State
    pub active_transfers: Vec<TransferTask>,
    pub last_finished_transfer: Option<FinishedTransfer>,
    pub prompt_input: String,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl App {
    pub async fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let config = Config::load().unwrap_or_default();
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
            active_transfers: Vec::new(),
            last_finished_transfer: None,
            prompt_input: String::new(),
            event_tx,
            event_rx,
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

    pub fn update_transfers(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::TransferProgress { id, bytes } => {
                    if let Some(task) = self.active_transfers.iter_mut().find(|t| t.id == id) {
                        task.bytes_transferred += bytes;
                    }
                }
                AppEvent::TransferFinished { id, result } => {
                    if let Some(task) = self.active_transfers.iter().find(|t| t.id == id) {
                        self.last_finished_transfer = Some(FinishedTransfer {
                            filename: task.filename.clone(),
                            target_path: task.target_path.clone(),
                            is_upload: task.is_upload,
                            result: result.clone(),
                        });
                    }
                    self.active_transfers.retain(|t| t.id != id);
                    if let Err(e) = result {
                        self.set_error(format!("Transfer failed: {}", e));
                    }
                }
            }
        }
    }

    pub async fn on_key(&mut self, key: crossterm::event::KeyCode) {
        if self.pending_delete_index.is_some() {
            match key {
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
             match key {
                crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Enter => {
                    self.error_message = None;
                    self.save_message = None;
                    return;
                }
                _ => {
                    self.error_message = None;
                    self.save_message = None;
                }
            }
        }

        match self.input_mode {
            InputMode::Normal => match key {
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
                crossterm::event::KeyCode::Char('d') => {
                    if self.active_pane == Pane::Main && (!self.selected_paths.is_empty() || self.main_list_state.selected().is_some()) {
                        self.input_mode = InputMode::DownloadPrompt;
                        self.prompt_input = String::new();
                    }
                }
                crossterm::event::KeyCode::Char('u') => {
                    if self.active_pane == Pane::Main && self.current_bucket.is_some() {
                        self.input_mode = InputMode::UploadPrompt;
                        self.prompt_input = String::new();
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
                                self.refresh_storage_providers().await;
                                let config_path = dirs::home_dir().unwrap().join(".config").join("stuie").join("config.json");
                                self.save_message = Some(format!("Configuration saved to {}!", config_path.display()));
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
                        let mut provider_to_activate = None;
                        if let Some(state) = &self.config_modal_state {
                            provider_to_activate = Some(state.selected_provider_index);
                        }

                        if let Some(idx) = provider_to_activate {
                            if let Some(provider) = self.storage_providers.get(idx) {
                                let provider_name = provider.name().to_string();
                                match provider.list_buckets().await {
                                    Ok(_) => {
                                        // Successful connection, refresh to ensure buckets are loaded
                                        self.refresh_storage_providers().await;
                                        self.active_provider_index = Some(idx);
                                        // Expand this provider
                                        self.expanded_providers.insert(idx);
                                        // Find first bucket of this provider to select it
                                        let items = self.sidebar_items();
                                        if let Some(item_idx) = items.iter().position(|item| {
                                            match item {
                                                SidebarItem::Bucket { provider_index, .. } => *provider_index == idx,
                                                _ => false,
                                            }
                                        }) {
                                            self.sidebar_state.select(Some(item_idx));
                                            self.save_message = Some(format!("Connection to '{}' successful! Buckets loaded.", provider_name));
                                            self.on_enter().await;
                                        } else {
                                            self.save_message = Some(format!("Connection to '{}' successful, but no buckets found.", provider_name));
                                        }
                                    }
                                    Err(e) => self.set_error(format!("Connection failed: {}", e)),
                                }
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
            InputMode::Editing => match key {
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
            InputMode::Search => match key {
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
            InputMode::DownloadPrompt => match key {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Enter => {
                    self.start_download().await;
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
            InputMode::UploadPrompt => match key {
                crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Enter => {
                    self.start_upload().await;
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
            InputMode::DeleteConfirmation => match key {
                crossterm::event::KeyCode::Char('y') | crossterm::event::KeyCode::Char('Y') => {
                    self.start_batch_delete().await;
                    self.input_mode = InputMode::Normal;
                }
                crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') | crossterm::event::KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                }
                _ => {}
            }
        }
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

        if let Some(provider) = self.storage_providers.get(provider_idx) {
            match provider.delete_objects(&bucket, &paths_to_delete).await {
                Ok(_) => {
                    self.save_message = Some(format!("Successfully deleted {} objects.", paths_to_delete.len()));
                    self.selected_paths.clear();
                    // Refresh objects
                    if let Some(provider) = self.storage_providers.get(provider_idx) {
                        if let Ok(objects) = provider.list_objects(&bucket, self.current_path.as_ref()).await {
                            self.objects = objects;
                            if !self.objects.is_empty() {
                                self.main_list_state.select(Some(0));
                            } else {
                                self.main_list_state.select(None);
                            }
                            self.update_preview().await;
                        }
                    }
                }
                Err(e) => self.set_error(format!("Failed to delete objects: {}", e)),
            }
        }
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
                                    self.expanded_providers.insert(*p_idx);
                                }
                            }
                            SidebarItem::Bucket { provider_index, name } => {
                                if let Some(provider) = self.storage_providers.get(*provider_index) {
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
                                    if !self.objects.is_empty() {
                                        self.main_list_state.select(Some(0));
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

        let mut paths_to_download = Vec::new();
        if !self.selected_paths.is_empty() {
            paths_to_download.extend(self.selected_paths.iter().cloned());
        } else if let Some(index) = self.main_list_state.selected() {
            let filtered = self.filtered_objects();
            if let Some(obj) = filtered.get(index) {
                if !obj.is_dir {
                    paths_to_download.push(obj.path.clone());
                }
            }
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

        for path in paths_to_download {
            let bucket_inner = bucket.clone();
            let filename = path.filename().unwrap_or(path.as_ref()).to_string();
            let local_path = local_base_path.join(&filename);
            let id = format!("download_{}_{}", bucket_inner, path);
            
            // Find total size
            let total_size = self.objects.iter().find(|o| o.path == path).map(|o| o.size).unwrap_or(0);

            let task = TransferTask {
                id: id.clone(),
                filename: filename.clone(),
                is_upload: false,
                bytes_transferred: 0,
                total_size,
                start_time: std::time::Instant::now(),
                target_path: local_path.to_string_lossy().to_string(),
            };
            self.active_transfers.push(task);

            let provider = self.storage_providers[provider_idx].clone_box();
            let event_tx = self.event_tx.clone();
            let task_id = id.clone();

            tokio::spawn(async move {
                let event_tx_inner = event_tx.clone();
                let task_id_inner = task_id.clone();
                let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                    let _ = event_tx_inner.send(AppEvent::TransferProgress { id: task_id_inner.clone(), bytes });
                });

                let result = provider.download(&bucket_inner, &path, &local_path, progress_cb).await;
                let _ = event_tx.send(AppEvent::TransferFinished { 
                    id: task_id, 
                    result: result.map_err(|e| e.to_string()) 
                });
            });
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

        let task = TransferTask {
            id: id.clone(),
            filename: filename.clone(),
            is_upload: true,
            bytes_transferred: 0,
            total_size,
            start_time: std::time::Instant::now(),
            target_path: remote_path.to_string(),
        };
        self.active_transfers.push(task);

        let provider = self.storage_providers[provider_idx].clone_box();
        let event_tx = self.event_tx.clone();
        let task_id = id.clone();

        tokio::spawn(async move {
            let event_tx_inner = event_tx.clone();
            let task_id_inner = task_id.clone();
            let progress_cb: ProgressCallback = Arc::new(move |bytes| {
                let _ = event_tx_inner.send(AppEvent::TransferProgress { id: task_id_inner.clone(), bytes });
            });

            let result = provider.upload(&bucket, &remote_path, &local_path, progress_cb).await;
            let _ = event_tx.send(AppEvent::TransferFinished { 
                id: task_id, 
                result: result.map_err(|e| e.to_string()) 
            });
        });
    }
}
