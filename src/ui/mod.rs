mod icons;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Table, Row, Cell, Wrap, WidgetRef},
    Frame,
};
use crate::app::{App, Pane, InputMode, TaskType, ConfigTab};
use crate::config::{KeymapConfig, SettingsConfig};

pub fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Top Bar
                Constraint::Min(0),    // Middle
                Constraint::Length(3), // Bottom Bar
                Constraint::Length(1), // Keybindings
            ]
            .as_ref(),
        )
        .split(f.area());

    // Top Bar
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
        .split(chunks[0]);

    let full_path = app.get_full_remote_path();
    let path_inner_width = top_chunks[0].width.saturating_sub(2) as usize; // minus borders
    let max_path_width = path_inner_width.saturating_sub(3); // space for " 📋"
    let shortened_path = shorten_path(&full_path, max_path_width);

    let path_block = Block::default().borders(Borders::ALL).title(" Path ");
    let path_para = Paragraph::new(format!(" {}", shortened_path)).block(path_block);
    f.render_widget(path_para, top_chunks[0]);

    // Render clipboard icon at right edge of path bar
    let path_icon_x = top_chunks[0].x + top_chunks[0].width.saturating_sub(3);
    let path_icon_y = top_chunks[0].y + 1;
    let path_icon_area = Rect::new(path_icon_x, path_icon_y, 2, 1);
    f.render_widget(Span::styled("📋", Style::default().fg(Color::Cyan)), path_icon_area);
    app.copy_button_area = Some(path_icon_area);

    let is_search = matches!(app.input_mode, InputMode::Search);
    let search_title = if is_search { " Filter (Esc to exit) " } else { " Filter ('/' to edit) " };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .title(search_title)
        .border_style(if is_search { Style::default().fg(Color::Yellow) } else { Style::default() })
        .border_type(if is_search { BorderType::Thick } else { BorderType::Plain });
    
    let search_text = if is_search {
        format!("{}|", app.search_query)
    } else if app.search_query.is_empty() {
        " (no filter)".to_string()
    } else {
        format!(" {}", app.search_query)
    };
    let search_para = Paragraph::new(search_text).block(search_block);
    f.render_widget(search_para, top_chunks[1]);

    // Middle Panes
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(20), // Sidebar
                Constraint::Percentage(45), // Main List
                Constraint::Percentage(35), // Preview
            ]
            .as_ref(),
        )
        .split(chunks[1]);

    // Sidebar
    let show_icons = app.config.settings.show_icons;
    let raw_sidebar_items = app.sidebar_items();
    let sidebar_items: Vec<ListItem> = raw_sidebar_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            match item {
                crate::app::SidebarItem::Provider(idx) => {
                    if let Some(provider) = app.storage_providers.get(*idx) {
                        let expand_icon = if app.expanded_providers.contains(idx) { "\u{25bc}" } else { "\u{25b6}" };
                        if show_icons {
                            let picon = icons::provider_icon(&provider.provider_type());
                            ListItem::new(Line::from(vec![
                                Span::raw(format!("{} ", expand_icon)),
                                Span::styled(format!("{} ", picon.symbol), Style::default().fg(picon.color)),
                                Span::raw(format!("{}", provider.name())),
                            ])).style(Style::default().add_modifier(Modifier::BOLD))
                        } else {
                            ListItem::new(format!("{} ({:?} - {})", expand_icon, provider.provider_type(), provider.name()))
                                .style(Style::default().add_modifier(Modifier::BOLD))
                        }
                    } else {
                        ListItem::new("Unknown Provider")
                    }
                }
                crate::app::SidebarItem::Bucket { name, .. } => {
                    let is_last = raw_sidebar_items.get(i + 1)
                        .map_or(true, |next| matches!(next, crate::app::SidebarItem::Provider(_)));
                    let branch = if is_last { "└─" } else { "├─" };
                    if show_icons {
                        ListItem::new(Line::from(vec![
                            Span::styled(format!(" {} ", branch), Style::default().fg(Color::DarkGray)),
                            Span::styled(format!("{} ", icons::ICON_BUCKET.symbol), Style::default().fg(icons::ICON_BUCKET.color)),
                            Span::styled(name.clone(), Style::default().fg(Color::Cyan)),
                        ]))
                    } else {
                        ListItem::new(Line::from(vec![
                            Span::styled(format!(" {} ", branch), Style::default().fg(Color::DarkGray)),
                            Span::styled(name.clone(), Style::default().fg(Color::Cyan)),
                        ]))
                    }
                }
            }
        })
        .collect();

    let is_sidebar_active = app.active_pane == Pane::Sidebar;
    let sidebar_block = Block::default()
        .borders(Borders::ALL)
        .title(" Buckets ")
        .border_style(if is_sidebar_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        })
        .border_type(if is_sidebar_active { BorderType::Thick } else { BorderType::Plain });

    let sidebar_list = List::new(sidebar_items)
        .block(sidebar_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray).fg(Color::Yellow))
        .highlight_symbol("> ");
    f.render_stateful_widget(sidebar_list, middle_chunks[0], &mut app.sidebar_state);

    // Main List and Preview
    {
        let filtered_objects = app.filtered_objects();
        
        // Render Main List Table
        let is_main_active = app.active_pane == Pane::Main;
        let is_normal_mode = matches!(app.input_mode, InputMode::Normal);
        
        let main_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Objects ({}) ", filtered_objects.len()))
            .border_style(if is_main_active && !is_normal_mode {
                Style::default()
            } else if is_main_active {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            })
            .border_type(if is_main_active && is_normal_mode { 
                BorderType::Thick 
            } else { 
                BorderType::Plain 
            });

        let header_cells = ["Name", "Size", "Modified"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = filtered_objects.iter().map(|obj| {
            let is_selected = app.selected_paths.contains(&obj.path);
            let checkbox = if is_selected { "[*]" } else { "[ ]" };

            let raw_name = obj.path.filename().unwrap_or_else(|| obj.path.as_ref()).to_string();
            let mut display_name = raw_name.clone();
            if obj.is_dir && !display_name.ends_with('/') {
                display_name.push('/');
            }

            let style = if obj.is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            let name_cell = if show_icons {
                let icon = icons::get_file_icon(&raw_name, obj.is_dir);
                Cell::from(Line::from(vec![
                    Span::styled(format!("{} ", checkbox), style),
                    Span::styled(format!("{} ", icon.symbol), Style::default().fg(icon.color)),
                    Span::styled(display_name, style),
                ]))
            } else {
                Cell::from(format!("{} {}", checkbox, display_name))
            };

            Row::new(vec![
                name_cell,
                Cell::from(format_size(obj.size)),
                Cell::from(obj.last_modified.format("%Y-%m-%d %H:%M").to_string()),
            ]).style(style)
        }).collect();

        let main_table = Table::new(rows, [
            Constraint::Percentage(50),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ])
        .header(header)
        .block(main_block)
        .row_highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray).fg(Color::Yellow))
        .highlight_symbol("> ");
        
        // Preview Pane preparation
        let preview_block = Block::default().borders(Borders::ALL).title(" Preview ");
        let preview_inner = preview_block.inner(middle_chunks[2]);
        let preview_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(0)].as_ref())
            .split(preview_inner);

        f.render_widget(preview_block, middle_chunks[2]);

        if let Some(index) = app.main_list_state.selected() {
            if let Some(obj) = filtered_objects.get(index) {
                let metadata = format!(
                    "Name: {}\nSize: {}\nType: {}\nModified: {}",
                    obj.path.filename().unwrap_or(obj.path.as_ref()),
                    format_size(obj.size),
                    if obj.is_dir { "Folder" } else { "File" },
                    obj.last_modified.format("%Y-%m-%d %H:%M:%S")
                );
                let metadata_para = Paragraph::new(metadata).style(Style::default().fg(Color::Cyan));
                f.render_widget(metadata_para, preview_chunks[0]);

                if !obj.is_dir {
                    if let Some(data) = &app.preview_data {
                        let content = if is_text(data) {
                            String::from_utf8_lossy(data).to_string()
                        } else {
                            format_hex(data)
                        };
                        let content_para = Paragraph::new(content)
                            .wrap(Wrap { trim: true })
                            .block(Block::default().borders(Borders::TOP).title(" Content Preview "));
                        f.render_widget(content_para, preview_chunks[1]);
                    } else {
                        let loading_para = Paragraph::new("Loading preview...")
                            .block(Block::default().borders(Borders::TOP).title(" Content Preview "));
                        f.render_widget(loading_para, preview_chunks[1]);
                    }
                }
            }
        }
        
        f.render_stateful_widget(main_table, middle_chunks[1], &mut app.main_list_state);
    }

    // Bottom Bar
    let keybindings = if app.show_config_modal {
        let active_tab = app.config_modal_state.as_ref().map(|s| s.active_tab);
        let is_editing = matches!(app.input_mode, InputMode::Editing);
        match (active_tab, is_editing) {
            (Some(ConfigTab::Providers), true) => {
                " Next/Prev Field: Tab/S-Tab | Finish Editing: Enter/Esc | Type to Input | Delete: Backspace | Type/HTTP: j/k "
            }
            (Some(ConfigTab::Providers), false) => {
                " Switch Tab: Tab/S-Tab | Navigate: j/k | Edit: e/Enter | Add: a | Delete: D | Save: s | Toggle: Enter | Close: q "
            }
            (Some(ConfigTab::Keymap), true) => {
                " Type new key | Confirm: Enter | Cancel: Esc "
            }
            (Some(ConfigTab::Keymap), false) => {
                " Switch Tab: Tab/S-Tab | Navigate: j/k | Edit: e/Enter | Save: s | Close: q "
            }
            (Some(ConfigTab::Settings), true) => {
                " Type value | Confirm: Enter | Cancel: Esc "
            }
            (Some(ConfigTab::Settings), false) => {
                " Switch Tab: Tab/S-Tab | Navigate: j/k | Toggle: Enter | Save: s | Close: q "
            }
            _ => " Save: s | Close: q "
        }
    } else if matches!(app.input_mode, InputMode::OverwriteConfirmation) {
        " Overwrite: o | Skip: s | Rename: r | Overwrite All: O | Skip All: S | Cancel: Esc "
    } else {
        " Quit: q | Help: ? | Filter: / | Select: Space | Copy Path: y | New Folder: n | Download: d | Upload: u | Delete: x | Config: c | Switch: Tab "
    };
    let status_text = if let Some(last_task) = app.active_tasks.last() {
        match last_task.task_type {
            TaskType::Upload | TaskType::Download => {
                let percent = if last_task.total_size > 0 {
                    (last_task.bytes_transferred * 100) / last_task.total_size
                } else {
                    0
                };
                let elapsed = last_task.start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    last_task.bytes_transferred as f64 / elapsed
                } else {
                    0.0
                };
                format!(
                    " [{}] {}: [{}%] ({}/s)",
                    if last_task.task_type == TaskType::Upload { "Upload" } else { "Download" },
                    last_task.description,
                    percent,
                    format_size(speed as u64)
                )
            }
            TaskType::CreateFolder => format!(" [New Folder] {}", last_task.description),
            TaskType::Delete => format!(" [Delete] {}", last_task.description),
        }
    } else if let Some(finished) = &app.last_finished_task {
        let result_str = match &finished.result {
            Ok(_) => "Done",
            Err(e) => e,
        };
        format!(
            " [{}] {}: {}",
            match finished.task_type {
                TaskType::Upload => "Upload",
                TaskType::Download => "Download",
                TaskType::CreateFolder => "New Folder",
                TaskType::Delete => "Delete",
            },
            finished.description,
            result_str
        )
    } else {
        " Ready".to_string()
    };

    let task_inner_width = chunks[2].width.saturating_sub(2) as usize; // minus borders
    let max_status_width = task_inner_width.saturating_sub(3); // space for " 📋"
    let truncated_status = if status_text.len() > max_status_width {
        format!("{}…", &status_text[..max_status_width.saturating_sub(1)])
    } else {
        status_text
    };

    let bottom_bar = Paragraph::new(truncated_status)
        .block(Block::default()
            .title(" Task ")
            .borders(Borders::ALL)
        );
    f.render_widget(bottom_bar, chunks[2]);

    // Render clipboard icon at right edge of task bar
    let task_icon_x = chunks[2].x + chunks[2].width.saturating_sub(3);
    let task_icon_y = chunks[2].y + 1;
    let task_icon_area = Rect::new(task_icon_x, task_icon_y, 2, 1);
    f.render_widget(Span::styled("📋", Style::default().fg(Color::Cyan)), task_icon_area);
    app.task_copy_area = Some(task_icon_area);

    let keybindings_para = Paragraph::new(keybindings)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(keybindings_para, chunks[3]);

    // Modals
    if app.show_config_modal {
        render_config_modal(f, app);
    }

    match app.input_mode {
        InputMode::DownloadPrompt => {
            render_file_explorer(f, app, " Download to ");
        }
        InputMode::UploadPrompt => {
            render_file_explorer(f, app, " Select file to upload (Enter: File | F: Upload Folder) ");
        }
        InputMode::DeleteConfirmation => {
            render_delete_confirmation(f, app);
        }
        InputMode::CreateFolderPrompt => {
            render_prompt(f, " New Folder Name ", &app.prompt_input);
        }
        InputMode::OverwriteConfirmation => {
            render_overwrite_confirmation(f, app);
        }
        _ => {}
    }

    if let Some(index) = app.pending_delete_index {
        let area = centered_rect(40, 20, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Confirm Delete ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        let text = format!("Are you sure you want to delete provider #{}?\n(y)es / (n)o", index + 1);
        let para = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::Red));
        f.render_widget(para, area);
    }

    if let Some(msg) = &app.save_message {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Success ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));
        let paragraph = Paragraph::new(msg.as_str())
            .block(block)
            .style(Style::default().fg(Color::Green));
        f.render_widget(paragraph, area);
    }

    if app.show_help {
        let area = centered_rect(80, 80, f.area());
        f.render_widget(Clear, area);

        let (help_title, help_text) = if app.show_config_modal {
            if matches!(app.input_mode, InputMode::Editing) {
                (" Help (Editing Mode) ", "\
\nField Navigation:
  Tab       : Next field
  S-Tab     : Previous field

Editing:
  Type      : Input text into the selected field
  Backspace : Delete character
  Opt+Bksp  : Delete previous word
  Cmd+Bksp  : Clear line
  Enter     : Finish editing
  Esc       : Cancel editing

Special Fields:
  j/k or \u{2191}/\u{2193} : Cycle options (Type / Allow HTTP)
")
            } else {
                (" Help (Configuration) ", "\
\nTabs:
  Tab/S-Tab  : Switch between Providers / Keymap / Settings

Providers Tab:
  j/k        : Navigate providers
  e / Enter  : Edit provider fields
  Enter      : Toggle enabled/disabled
  a          : Add new provider
  D          : Delete selected provider

Keymap Tab:
  j/k        : Navigate keybindings
  e / Enter  : Edit selected keybinding

Settings Tab:
  j/k        : Navigate settings
  Enter      : Toggle boolean settings
  e          : Edit numeric settings

General:
  s          : Save configuration
  q / Esc    : Close without saving
  ?          : Toggle Help
")
            }
        } else {
            (" Help / Keybindings ", "\
\nNavigation:
  j/k       : Move selection down/up
  Tab       : Switch between Sidebar and Main List
  Enter     : Enter bucket or folder
  Backspace : Go back to parent / bucket list
  /         : Enter Filter mode

Actions:
  Space     : Select/Deselect object
  y         : Copy current path to clipboard
  n         : Create new folder
  d         : Download selected objects
  u         : Upload file or folder to current path
  x/Delete  : Delete selected objects
  c         : Toggle Configuration
  ?         : Toggle Help
  q / Esc   : Quit / Close Modal
")
        };

        let help_block = Block::default().title(help_title).borders(Borders::ALL);
        let help_para = Paragraph::new(help_text).block(help_block);
        f.render_widget(help_para, area);
    }

    if let Some(error) = &app.error_message {
        let area = centered_rect(60, 40, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        
        let inner_area = block.inner(area);
        f.render_widget(block, area);

        let error_line = Line::from(vec![
            Span::styled("📋", Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::raw(error.as_str()),
        ]);
        
        let paragraph = Paragraph::new(error_line)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Red));
        
        f.render_widget(paragraph, inner_area);

        // Save error copy area (icon is at start of inner area)
        app.error_copy_area = Some(Rect::new(inner_area.x, inner_area.y, 2, 1));
    }
}

fn render_prompt(f: &mut Frame, title: &str, input: &str) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let para = Paragraph::new(format!("{}|", input)).block(block);
    f.render_widget(para, area);
}

fn render_delete_confirmation(f: &mut Frame, app: &App) {
    let mut paths_to_delete = Vec::new();
    if !app.selected_paths.is_empty() {
        paths_to_delete.extend(app.selected_paths.iter().map(|p| p.to_string()));
    } else if let Some(index) = app.main_list_state.selected() {
        let filtered = app.filtered_objects();
        if let Some(obj) = filtered.get(index) {
            paths_to_delete.push(obj.path.to_string());
        }
    }

    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Confirm Batch Delete ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));
    
    let list_items: Vec<ListItem> = paths_to_delete.iter()
        .map(|p| ListItem::new(format!("  - {}", p)))
        .collect();
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
        .split(block.inner(area));
    
    f.render_widget(block, area);
    
    let list = List::new(list_items).block(Block::default().title(" Items to delete ").borders(Borders::BOTTOM));
    f.render_widget(list, chunks[0]);
    
    let confirm_text = format!("Delete {} items? (y)es / (n)o", paths_to_delete.len());
    let confirm_para = Paragraph::new(confirm_text).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));
    f.render_widget(confirm_para, chunks[1]);
}

fn render_overwrite_confirmation(f: &mut Frame, app: &App) {
    let state = match &app.conflict_state {
        Some(s) => s,
        None => return,
    };

    let conflict = match state.conflicts.get(state.current_index) {
        Some(c) => c,
        None => return,
    };

    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);

    let title = format!(
        " File Conflict ({}/{}) ",
        state.current_index + 1,
        state.conflicts.len()
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // File info
            Constraint::Min(0),    // Options
            Constraint::Length(1), // Hint
        ])
        .split(inner);

    let info_text = format!(
        "\"{}\" already exists at:\n{}",
        conflict.source_name,
        conflict.target_display,
    );
    let info_para = Paragraph::new(info_text)
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: true });
    f.render_widget(info_para, chunks[0]);

    let options = [
        ("o", "Overwrite"),
        ("s", "Skip"),
        ("r", "Rename (auto-number)"),
        ("O", "Overwrite All"),
        ("S", "Skip All"),
    ];

    let option_items: Vec<ListItem> = options.iter().enumerate().map(|(i, (key, label))| {
        let is_selected = i == state.selected_option;
        let prefix = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        ListItem::new(Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("[{}] ", key), Style::default().fg(Color::Cyan)),
            Span::styled(*label, style),
        ]))
    }).collect();

    let options_list = List::new(option_items)
        .block(Block::default().borders(Borders::TOP).title(" Choose action "));
    f.render_widget(options_list, chunks[1]);

    let hint = Paragraph::new(" Enter: confirm | Esc: cancel")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, chunks[2]);
}

fn render_config_modal(f: &mut Frame, app: &mut App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Configuration ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);

    let inner_area = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 3);

    if let Some(state) = &app.config_modal_state {
        // Tab bar + content layout
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .split(inner_area);

        // Render tab bar
        let tab_spans: Vec<Span> = ConfigTab::ALL.iter().map(|tab| {
            if *tab == state.active_tab {
                Span::styled(
                    format!(" {} ", tab.label()),
                    Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!(" {} ", tab.label()),
                    Style::default().fg(Color::DarkGray),
                )
            }
        }).collect();
        let tab_line = Paragraph::new(Line::from(tab_spans));
        f.render_widget(tab_line, main_chunks[0]);

        let content_area = main_chunks[2];

        match state.active_tab {
            ConfigTab::Providers => render_config_providers(f, state, content_area),
            ConfigTab::Keymap => render_config_keymap(f, state, content_area),
            ConfigTab::Settings => render_config_settings(f, state, content_area),
        }
    }
}

fn render_config_providers(f: &mut Frame, state: &crate::app::ConfigModalState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(area);

    // Providers List (Left)
    let providers: Vec<ListItem> = state.providers.iter().enumerate().map(|(i, p)| {
        let status = if p.enabled { "[ON]" } else { "[OFF]" };
        let label = format!("{}. {} {}", i + 1, if p.name.is_empty() { "(unnamed)" } else { &p.name }, status);

        let style = if i == state.selected_provider_index && !state.edit_mode {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if i == state.selected_provider_index {
            Style::default().fg(Color::Yellow)
        } else if p.enabled {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        ListItem::new(label).style(style)
    }).collect();
    let providers_list = List::new(providers)
        .block(Block::default().title(" Providers (j/k) ").borders(Borders::ALL));
    f.render_widget(providers_list, chunks[0]);

    // Fields (Right)
    if let Some(provider) = state.providers.get(state.selected_provider_index) {
        let fields_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), Constraint::Length(3), Constraint::Length(3),
                Constraint::Length(3), Constraint::Length(3), Constraint::Length(3),
                Constraint::Length(3), Constraint::Length(3),
            ].as_ref())
            .split(chunks[1]);

        let field_names = [
            "Name *", "Type *", "Region *", "Bucket Name (Optional)",
            "Endpoint (Optional)", "Access Key *", "Secret Key *", "Allow HTTP (Optional)"
        ];

        for i in 0..8 {
            let is_selected = state.edit_mode && state.selected_field_index == i;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default()
            };

            let mut value = match i {
                0 => provider.name.clone(),
                1 => format!("{:?}", provider.provider_type),
                2 => provider.region.clone().unwrap_or_default(),
                3 => provider.bucket_name.clone().unwrap_or_default(),
                4 => provider.endpoint.clone().unwrap_or_default(),
                5 => provider.access_key_id.clone().unwrap_or_default(),
                6 => provider.secret_access_key.clone().unwrap_or_default(),
                7 => provider.allow_http.unwrap_or(false).to_string(),
                _ => String::new(),
            };

            let title = if is_selected {
                format!("{} [EDITING]", field_names[i])
            } else {
                field_names[i].to_string()
            };

            if is_selected {
                if i == 1 {
                     value = format!("[ S3 | GCS | Azure ] - Current: {:?}", provider.provider_type);
                } else if i == 7 {
                     value = format!("[ true | false ] - Current: {}", provider.allow_http.unwrap_or(false));
                } else {
                     value.push('_');
                }
            }

            let field_block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(style);
            let field_para = Paragraph::new(value).block(field_block);
            f.render_widget(field_para, fields_chunks[i]);
        }
    }
}

fn render_config_keymap(f: &mut Frame, state: &crate::app::ConfigModalState, area: Rect) {
    let block = Block::default().title(" Keybindings ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut items: Vec<ListItem> = Vec::new();

    for i in 0..KeymapConfig::FIELD_COUNT {
        // Insert section header if applicable
        if let Some(header) = KeymapConfig::section_header(i) {
            let header_style = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
            if i > 0 {
                items.push(ListItem::new(Line::raw(""))); // blank separator
            }
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("  {} ", header), header_style),
                Span::styled("─".repeat(30), Style::default().fg(Color::DarkGray)),
            ])));
        }

        let is_selected = i == state.selected_keymap_index;
        let name = KeymapConfig::field_name(i);
        let value = if state.keymap_editing && is_selected {
            format!("{}|", state.keymap_input)
        } else {
            state.keymap.get_field(i).to_string()
        };

        let style = if state.keymap_editing && is_selected {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let prefix = if is_selected { "> " } else { "  " };
        items.push(ListItem::new(Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("  {:<16}", name), style),
            Span::styled(format!("  {}", value), if state.keymap_editing && is_selected {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::Cyan)
            }),
        ])));
    }

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn render_config_settings(f: &mut Frame, state: &crate::app::ConfigModalState, area: Rect) {
    let block = Block::default().title(" Settings ").borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = (0..SettingsConfig::FIELD_COUNT).map(|i| {
        let is_selected = i == state.selected_setting_index;
        let name = SettingsConfig::field_name(i);
        let value = match i {
            0 => if state.settings.show_icons { "true" } else { "false" }.to_string(),
            1 => if state.settings.confirm_delete { "true" } else { "false" }.to_string(),
            2 => state.settings.preview_max_bytes.to_string(),
            _ => String::new(),
        };

        let style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let hint = match i {
            0 | 1 => " (Enter to toggle)",
            2 => " (e to edit)",
            _ => "",
        };

        let prefix = if is_selected { "> " } else { "  " };
        ListItem::new(Line::from(vec![
            Span::raw(prefix),
            Span::styled(format!("{:<20}", name), style),
            Span::styled(format!("  {}", value), Style::default().fg(Color::Cyan)),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ]))
    }).collect();

    let list = List::new(items);
    f.render_widget(list, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

fn is_text(data: &[u8]) -> bool {
    for &b in data {
        if b == 0 {
            return false;
        }
        if b < 32 && !b" \t\n\r".contains(&b) {
            return false;
        }
    }
    true
}

fn format_hex(data: &[u8]) -> String {
    let mut lines = Vec::new();
    for chunk in data.chunks(16) {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        let ascii: String = chunk.iter().map(|&b| {
            if b >= 32 && b <= 126 {
                b as char
            } else {
                '.'
            }
        }).collect();
        lines.push(format!("{:48} | {}", hex.join(" "), ascii));
    }
    lines.join("\n")
}

fn render_file_explorer(f: &mut Frame, app: &App, title: &str) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    if app.file_explorer_search_active || !app.file_explorer_filter.is_empty() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(inner_area);

        let search_style = if app.file_explorer_search_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let search_text = if app.file_explorer_search_active {
            format!(" /{}|", app.file_explorer_filter)
        } else {
            format!(" /{}", app.file_explorer_filter)
        };
        let search_para = Paragraph::new(search_text).style(search_style);
        f.render_widget(search_para, chunks[0]);

        app.file_explorer.widget().render_ref(chunks[1], f.buffer_mut());
    } else {
        app.file_explorer.widget().render_ref(inner_area, f.buffer_mut());
    }
}

fn shorten_path(full_path: &str, max_width: usize) -> String {
    if full_path.len() <= max_width {
        return full_path.to_string();
    }

    // Split into protocol+bucket and folders
    // Expected format: protocol://bucket/path/to/object
    let parts: Vec<&str> = full_path.splitn(4, '/').collect();
    if parts.len() < 4 {
        return full_path.to_string();
    }

    let protocol = parts[0]; // e.g. "s3:"
    let bucket = parts[2];   // e.g. "mybucket"
    let protocol_bucket = format!("{}//{}", protocol, bucket);
    let rest = parts[3];
    
    let mut path_parts: Vec<&str> = rest.split('/').collect();
    
    // Replace from left to right with ".." until it fits
    for i in 0..path_parts.len().saturating_sub(1) {
        path_parts[i] = "..";
        let new_path = format!("{}/{}", protocol_bucket, path_parts.join("/"));
        if new_path.len() <= max_width {
            return new_path;
        }
    }

    // If still too long, return what we have
    format!("{}/{}", protocol_bucket, path_parts.join("/"))
}
