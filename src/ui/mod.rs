use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Table, Row, Cell, Wrap},
    Frame,
};
use crate::app::{App, Pane, InputMode};

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

    let current_path_str = app.current_path.as_ref().map(|p| p.to_string()).unwrap_or_else(|| "/".to_string());
    let path_block = Block::default().borders(Borders::ALL).title(" Path ");
    let path_para = Paragraph::new(format!(" {}", current_path_str)).block(path_block);
    f.render_widget(path_para, top_chunks[0]);

    let is_search = matches!(app.input_mode, InputMode::Search);
    let search_title = if is_search { " Filter (Esc to exit) " } else { " Filter ('/' to edit) " };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .title(search_title)
        .border_style(if is_search { Style::default().fg(Color::Yellow) } else { Style::default() });
    
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
    let sidebar_items: Vec<ListItem> = app.sidebar_items()
        .iter()
        .map(|item| {
            match item {
                crate::app::SidebarItem::Provider(idx) => {
                    if let Some(provider) = app.storage_providers.get(*idx) {
                        let icon = if app.expanded_providers.contains(idx) { "\u{25bc}" } else { "\u{25b6}" };
                        ListItem::new(format!("{} ({:?} - {})", icon, provider.provider_type(), provider.name()))
                            .style(Style::default().add_modifier(Modifier::BOLD))
                    } else {
                        ListItem::new("Unknown Provider")
                    }
                }
                crate::app::SidebarItem::Bucket { name, .. } => {
                    ListItem::new(format!("  \u{25cf} {}", name))
                }
            }
        })
        .collect();

    let sidebar_block = Block::default()
        .borders(Borders::ALL)
        .title(" Buckets ")
        .border_style(if app.active_pane == Pane::Sidebar {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        });

    let sidebar_list = List::new(sidebar_items)
        .block(sidebar_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
        .highlight_symbol("> ");
    f.render_stateful_widget(sidebar_list, middle_chunks[0], &mut app.sidebar_state);

    // Main List and Preview
    {
        let filtered_objects = app.filtered_objects();
        
        // Render Main List Table
        let main_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Objects ({}) ", filtered_objects.len()))
            .border_style(if app.active_pane == Pane::Main && !matches!(app.input_mode, InputMode::Normal) {
                Style::default()
            } else if app.active_pane == Pane::Main {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            });

        let header_cells = ["Name", "Size", "Modified"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().add_modifier(Modifier::BOLD)));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows: Vec<Row> = filtered_objects.iter().map(|obj| {
            let is_selected = app.selected_paths.contains(&obj.path);
            let checkbox = if is_selected { "[*]" } else { "[ ]" };
            
            let mut display_name = obj.path.filename().unwrap_or_else(|| obj.path.as_ref()).to_string();
            if obj.is_dir && !display_name.ends_with('/') {
                display_name.push('/');
            }
            
            let name = format!("{} {}", checkbox, display_name);
            
            Row::new(vec![
                Cell::from(name),
                Cell::from(format_size(obj.size)),
                Cell::from(obj.last_modified.format("%Y-%m-%d %H:%M").to_string()),
            ])
        }).collect();

        let main_table = Table::new(rows, [
            Constraint::Percentage(50),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ])
        .header(header)
        .block(main_block)
        .row_highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray))
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
    let keybindings = " Help: ? | Filter: / | Select: Space | Download: d | Upload: u | Delete: x | Config: c | Switch: Tab ";
    let status_text = if let Some(last_transfer) = app.active_transfers.last() {
        let percent = if last_transfer.total_size > 0 {
            (last_transfer.bytes_transferred * 100) / last_transfer.total_size
        } else {
            0
        };
        let speed = last_transfer.bytes_transferred as f64 / last_transfer.start_time.elapsed().as_secs_f64();
        format!(
            " [{}] {}: [{}%] ({}/s)",
            if last_transfer.is_upload { "Upload" } else { "Download" },
            last_transfer.filename,
            percent,
            format_size(speed as u64)
        )
    } else if let Some(finished) = &app.last_finished_transfer {
        match &finished.result {
            Ok(_) => {
                let transfer_type = if finished.is_upload { "Upload" } else { "Download" };
                format!(" Finished {}: {} to {}", transfer_type, finished.filename, finished.target_path)
            }
            Err(e) => format!(" Failed: {} ({})", finished.filename, e),
        }
    } else {
        " Ready".to_string()
    };

    let bottom_bar = Paragraph::new(status_text)
        .block(Block::default()
            .title(" Task ")
            .borders(Borders::ALL)
        );
    f.render_widget(bottom_bar, chunks[2]);

    let keybindings_para = Paragraph::new(keybindings)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(keybindings_para, chunks[3]);

    // Modals
    if app.show_config_modal {
        render_config_modal(f, app);
    }

    match app.input_mode {
        InputMode::DownloadPrompt => {
            render_prompt(f, " Download to (local path) ", &app.prompt_input);
        }
        InputMode::UploadPrompt => {
            render_prompt(f, " Upload from (local path) ", &app.prompt_input);
        }
        InputMode::DeleteConfirmation => {
            render_delete_confirmation(f, app);
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
        let help_block = Block::default().title(" Help / Keybindings ").borders(Borders::ALL);
        let help_text = "
Navigation:
  j/k       : Move selection down/up
  Tab       : Switch between Sidebar and Main List
  Enter     : Enter bucket or folder
  Backspace : Go back to parent / bucket list
  /         : Enter Filter mode

Actions:
  Space     : Select/Deselect object
  d         : Download selected objects
  u         : Upload local file to current folder
  x/Delete  : Delete selected objects
  c         : Toggle Configuration
  ?         : Toggle Help
  q / Esc   : Quit / Close Modal
";
        let help_para = Paragraph::new(help_text).block(help_block);
        f.render_widget(help_para, area);
    }

    if let Some(error) = &app.error_message {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        let paragraph = Paragraph::new(error.as_str())
            .block(block)
            .style(Style::default().fg(Color::Red));
        f.render_widget(paragraph, area);
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

fn render_config_modal(f: &mut Frame, app: &mut App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Configuration ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block, area);

    let inner_area = Rect::new(area.x + 2, area.y + 2, area.width - 4, area.height - 4);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(inner_area);

    if let Some(state) = &app.config_modal_state {
        // Providers List (Left)
        let providers: Vec<ListItem> = state.providers.iter().enumerate().map(|(i, p)| {
            let is_active = app.active_provider_index == Some(i);
            let mut label = format!("{}. {}", i + 1, if p.name.is_empty() { "(unnamed)" } else { &p.name });
            if is_active {
                label = format!("{} [ACTIVE]", label);
            }
            
            let style = if i == state.selected_provider_index && !state.edit_mode {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if i == state.selected_provider_index {
                Style::default().fg(Color::Yellow)
            } else if is_active {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
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
                    Constraint::Length(3), // Name
                    Constraint::Length(3), // Type
                    Constraint::Length(3), // Region
                    Constraint::Length(3), // Bucket
                    Constraint::Length(3), // Endpoint
                    Constraint::Length(3), // Access Key
                    Constraint::Length(3), // Secret Key
                    Constraint::Length(3), // Allow HTTP
                    Constraint::Min(0),    // Hints
                ].as_ref())
                .split(chunks[1]);

            let field_names = [
                "Name *", 
                "Type *", 
                "Region *", 
                "Bucket Name (Optional)", 
                "Endpoint (Optional)", 
                "Access Key *", 
                "Secret Key *", 
                "Allow HTTP (Optional)"
            ];

            for i in 0..8 {
                let is_selected = state.edit_mode && state.selected_field_index == i;
                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else if !state.edit_mode && i == 0 { // Just some highlight if not editing
                    Style::default()
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
                    // Show all options for "Type" field when selected
                    if i == 1 {
                         value = format!("[ S3 | GCS | Azure ] - Current: {:?}", provider.provider_type);
                    } else if i == 7 {
                         value = format!("[ true | false ] - Current: {}", provider.allow_http.unwrap_or(false));
                    } else {
                         value.push('_'); // Cursor
                    }
                }

                let field_block = Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(style);
                let field_para = Paragraph::new(value).block(field_block);
                f.render_widget(field_para, fields_chunks[i]);
            }

            let hints = "
[Normal Mode]
  j/k or \u{2191}/\u{2193}: Navigate providers |  e/Enter: Edit fields
  a: Add provider          |  D: Delete provider
  s: Save configuration    |  Enter (on list): Activate
  c: Close without saving

[Editing Mode]
  Tab / S-Tab: Next/Prev field |  Enter/Esc: Finish editing
  Type to input                |  Backspace: Delete
  (Type/HTTP: cycle with j/k or \u{2191}/\u{2193})
";
            let hints_para = Paragraph::new(hints).block(Block::default().title(" Help ").borders(Borders::ALL));
            f.render_widget(hints_para, fields_chunks[8]);
        }
    }
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
