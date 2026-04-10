use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use stuie::app::App;
use stuie::ui::ui;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    let file_appender = tracing_appender::rolling::daily("logs", "stuie.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    tracing::info!("Starting stuie...");

    // Setup terminal
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).context("Failed to enter alternate screen and enable mouse capture")?;
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
    ).context("Failed to leave alternate screen and disable mouse capture")?;
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
        println!("Efficiency level Master: You did absolutely nothing this session. Zero tasks, zero worries! 🧘");
    } else {
        println!("Tasks completed this session:");
        for task in &app.session_tasks {
            let status = match &task.result {
                Ok(_) => "✅ Success",
                Err(e) => &format!("❌ Failed: {}", e),
            };
            println!("  - {}: {}", task.description, status);
        }
    }
    println!();

    // Final appreciation
    println!("Thank you for using stuie! We appreciate your support.");
    println!("Found a bug? Help us improve at: https://github.com/thmswt/stuie/issues");
    println!();
}

async fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        app.update_tasks().await;

        terminal.draw(|f| ui(f, app)).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if app.should_quit {
            return Ok(());
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            let ev = event::read()?;
            app.on_event(&ev).await;
        }
    }
}
