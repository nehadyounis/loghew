mod app;
mod config;
mod event;
mod log;
mod search;
mod ui;

use std::io;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use log::LogSource;

#[derive(Parser)]
#[command(name = "loghew", about = "A modern TUI log viewer")]
struct Cli {
    /// Log file to open (omit for stdin). Use +N to jump to line N.
    file: Option<String>,

    /// Additional positional: +N to jump to line
    extra: Option<String>,

    /// Initial search pattern
    #[arg(short = 's', long)]
    search: Option<String>,

    /// Jump to line on open
    #[arg(long)]
    line: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let (file_path, jump_line) = parse_file_arg(&cli);

    let is_tty = io::stdin().is_terminal();

    let (source, filename, log_path) = if let Some(path) = file_path {
        let p = PathBuf::from(&path);
        let source = LogSource::open(&p)?;
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(path);
        let abs = std::fs::canonicalize(&p).unwrap_or(p);
        (source, name, Some(abs))
    } else if !is_tty {
        let source = LogSource::open_stdin()?;
        (source, "stdin".to_string(), None)
    } else {
        anyhow::bail!("Usage: loghew <file> or pipe input via stdin");
    };

    let config = config::Config::load();
    let mouse_enabled = config.mouse;
    let mut app = App::new(source, filename, log_path, config);

    if let Some(line) = jump_line.or(cli.line) {
        app.scroll_to(line.saturating_sub(1));
    }

    if let Some(pattern) = &cli.search {
        app.search.set_literal(pattern);
        let source = &app.source;
        let total = source.index().total_lines;
        app.search.find_matches(total, |i| source.get_line(i));
        if let Some(line) = app.search.jump_to_nearest(0) {
            app.scroll_to(line);
        }
    }

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), crossterm::event::DisableMouseCapture);
        std::thread::sleep(std::time::Duration::from_millis(50));
        while crossterm::event::poll(std::time::Duration::from_millis(10)).unwrap_or(false) {
            let _ = crossterm::event::read();
        }
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if mouse_enabled {
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        )?;
    } else {
        execute!(stdout, EnterAlternateScreen)?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut needs_redraw = true;
    let mut prev_mode: u8 = 0;
    loop {
        if needs_redraw {
            let mode = if app.show_help { 1 }
                else if app.show_config { 2 }
                else if app.show_bookmarks { 3 }
                else if app.show_notifications { 4 }
                else { 0 };
            if mode != prev_mode {
                execute!(
                    terminal.backend_mut(),
                    crossterm::terminal::BeginSynchronizedUpdate,
                )?;
                terminal.clear()?;
                terminal.draw(|f| ui::draw(f, &mut app))?;
                execute!(
                    terminal.backend_mut(),
                    crossterm::terminal::EndSynchronizedUpdate,
                )?;
                prev_mode = mode;
            } else {
                terminal.draw(|f| ui::draw(f, &mut app))?;
            }
            needs_redraw = false;
        }
        let had_event = event::handle_event(&mut app)?;
        if had_event {
            needs_redraw = true;
            // Drain pending events intelligently:
            // - Drag: batch all (only final position matters for selection)
            // - Scroll: DISCARD extras (prevents runaway, first one already handled)
            // - Other: stop draining
            while !app.should_quit
                && crossterm::event::poll(std::time::Duration::from_millis(0))?
            {
                let evt = crossterm::event::read()?;
                if event::is_drag(&evt) {
                    event::dispatch(&mut app, &evt);
                } else if event::is_scroll(&evt) {
                    // Drop it — first scroll this frame was already processed
                } else {
                    event::dispatch(&mut app, &evt);
                    break;
                }
            }
        } else if app.searching() {
            app.search_tick();
            needs_redraw = true;
        } else if app.filtering() {
            app.filter_tick();
            needs_redraw = true;
        } else if app.is_scanning() {
            app.scan_tick();
            if app.follow_mode {
                app.scroll_to_bottom();
            }
            needs_redraw = true;
        } else if app.tick() {
            needs_redraw = true;
        } else if !app.indexing_ready() {
            app.parse_deferred_batch();
            needs_redraw = true;
        }
        if app.should_quit {
            break;
        }
    }

    // Disable mouse FIRST to stop the terminal generating new events
    if mouse_enabled {
        execute!(
            terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
        )?;
    }

    // Give the terminal time to process the disable command
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Aggressively drain any buffered mouse/key events
    while crossterm::event::poll(std::time::Duration::from_millis(10))? {
        let _ = crossterm::event::read();
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn parse_file_arg(cli: &Cli) -> (Option<String>, Option<usize>) {
    let mut file = cli.file.clone();
    let mut jump = None;

    if let Some(ref arg) = file {
        if let Some(rest) = arg.strip_prefix('+') {
            if let Ok(n) = rest.parse::<usize>() {
                jump = Some(n);
                file = None;
            }
        }
    }

    if let Some(ref arg) = cli.extra {
        if let Some(rest) = arg.strip_prefix('+') {
            if let Ok(n) = rest.parse::<usize>() {
                jump = Some(n);
            }
        }
    }

    (file, jump)
}
