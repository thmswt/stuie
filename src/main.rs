use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::fs;
use stuie::app::App;
use stuie::config::Config;
use stuie::ui::ui;

#[tokio::main]
async fn main() -> Result<()> {
    // Load config early so diagnostics settings can gate tracing init.
    let early_config = Config::load().unwrap_or_default();

    // Setup logging only when diagnostics are enabled. The non-blocking guard must
    // outlive any tracing calls, so we bind it to a let with a matching lifetime.
    let _log_guard = if early_config.settings.diagnostics_enabled {
        let log_dir = if early_config.settings.diagnostics_log_dir.trim().is_empty() {
            "logs".to_string()
        } else {
            early_config.settings.diagnostics_log_dir.clone()
        };
        let file_appender = tracing_appender::rolling::daily(&log_dir, "stuie.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::fmt()
            .with_writer(non_blocking)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(tracing::Level::INFO.into()),
            )
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_target(true)
            .init();
        tracing::info!(
            version = env!("CARGO_PKG_VERSION"),
            log_dir = %log_dir,
            "Starting stuie"
        );
        Some(guard)
    } else {
        None
    };

    // Setup terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate screen and enable mouse capture")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    // Create app and run it
    let mut app = App::new().await;
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("Failed to leave alternate screen and disable mouse capture")?;
    terminal.show_cursor().context("Failed to show cursor")?;

    if let Err(err) = res {
        eprintln!("{:?}", err);
    }

    // Print quit message
    print_quit_screen(&app);

    Ok(())
}

fn print_quit_screen(app: &App) {
    // Print ASCII Logo
    if let Ok(logo) = fs::read_to_string("logo.txt") {
        println!("{}", logo);
    } else {
        println!("   _____ _______ _    _ _____ ______");
        println!("  / ____|__   __| |  | |_   _|  ____|");
        println!(" | (___    | |  | |  | | | | | |__   ");
        println!("  \\___ \\   | |  | |  | | | | |  __|  ");
        println!("  ____) |  | |  | |__| |_| |_| |____ ");
        println!(" |_____/   |_|   \\____/|_____|______|");
        println!("     Storage Terminal UI Explorer");
    }
    println!();

    // Print Session Tasks
    if app.session_tasks.is_empty() {
        println!("You did absolutely nothing this session. Zero tasks, zero worries! 🧘");
    } else {
        let log_to_file = app.config.settings.log_to_file;
        let total = app.session_tasks.len();
        let console_tasks: Vec<_> = if log_to_file {
            app.session_tasks.iter().take(5).collect()
        } else {
            app.session_tasks.iter().collect()
        };

        println!("Tasks completed this session:");
        for task in &console_tasks {
            let status = match &task.result {
                Ok(_) => "✅ Success".to_string(),
                Err(e) => format!("❌ Failed: {}", e),
            };
            println!("  - {}: {}", task.description, status);
        }

        if log_to_file {
            let remaining = total.saturating_sub(console_tasks.len());
            if remaining > 0 {
                println!("  … {} more (see log file)", remaining);
            }
            match write_task_log(&app.config.settings.log_file_path, &app.session_tasks) {
                Ok(path) => println!("Full task log written to {}", path.display()),
                Err(e) => eprintln!("Failed to write task log: {}", e),
            }
        }
    }
    println!();

    // Final appreciation
    println!("Thank you for using stuie! We appreciate your support.");
    println!("Found a bug? Help us improve at: https://github.com/thmswt/stuie/issues");
    println!();
}

fn write_task_log(path: &str, tasks: &[stuie::app::FinishedTask]) -> Result<std::path::PathBuf> {
    use std::io::Write;

    let expanded = if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .context("home directory not found")?
            .join(rest)
    } else if path == "~" {
        dirs::home_dir().context("home directory not found")?
    } else {
        std::path::PathBuf::from(path)
    };

    if let Some(parent) = expanded.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).context("Failed to create log directory")?;
        }
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&expanded)
        .context("Failed to open log file")?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "=== Session {} ===", timestamp)?;
    for task in tasks {
        let status = match &task.result {
            Ok(_) => "Success".to_string(),
            Err(e) => format!("Failed: {}", e),
        };
        writeln!(file, "  - {}: {}", task.description, status)?;
    }
    writeln!(file)?;
    Ok(expanded)
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.update_tasks().await;

        terminal
            .draw(|f| ui(f, app))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if app.should_quit {
            return Ok(());
        }

        // Spawn any pending action as a background task so the main loop keeps redrawing
        if app.pending_action.is_some() {
            app.start_pending_action();
        }

        // Shorter poll timeout while loading so the spinner animates
        let timeout = if app.loading_message.is_some() {
            std::time::Duration::from_millis(80)
        } else {
            std::time::Duration::from_millis(100)
        };
        if event::poll(timeout)? {
            let ev = event::read()?;
            app.on_event(&ev).await;
        }
    }
}
