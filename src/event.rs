use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use crate::app::{App, InputMode};

pub fn handle_event(app: &mut App) -> anyhow::Result<bool> {
    if !event::poll(std::time::Duration::from_millis(50))? {
        return Ok(false);
    }
    let evt = event::read()?;
    dispatch(app, &evt);
    Ok(true)
}

pub fn dispatch(app: &mut App, evt: &Event) {
    match evt {
        Event::Key(key) => handle_key(app, *key),
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp if !app.follow_mode => app.scroll_up(app.config.scroll_speed),
            MouseEventKind::ScrollDown if !app.follow_mode => app.scroll_down(app.config.scroll_speed),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse.modifiers.contains(KeyModifiers::ALT) {
                    app.ctrl_click_line(mouse.row);
                } else if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                    app.shift_click_line(mouse.row);
                } else {
                    let now = std::time::Instant::now();
                    let is_double = app.last_click.map_or(false, |(t, r)| {
                        r == mouse.row && now.duration_since(t).as_millis() < 400
                    });
                    if is_double {
                        app.double_click_line(mouse.row);
                        app.last_click = None;
                    } else {
                        app.click_line(mouse.row);
                        app.start_drag(mouse.row, mouse.column);
                        app.last_click = Some((now, mouse.row));
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                app.update_drag(mouse.row, mouse.column);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                app.end_drag();
            }
            _ => {}
        },
        _ => {}
    }
}

pub fn is_drag(evt: &Event) -> bool {
    matches!(evt, Event::Mouse(m) if matches!(m.kind, MouseEventKind::Drag(_)))
}

pub fn is_scroll(evt: &Event) -> bool {
    matches!(evt, Event::Mouse(m) if matches!(m.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown))
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        || key.modifiers.contains(KeyModifiers::SUPER)
    {
        match key.code {
            KeyCode::Char('c') => {
                if let Some(text) = app.selected_text() {
                    let is_text = app.text_selection.is_some();
                    let count = if is_text {
                        let lines = text.lines().count();
                        if lines <= 1 {
                            format!("{} chars", text.len())
                        } else {
                            format!("{} lines", lines)
                        }
                    } else {
                        let n = app.selected_lines.len();
                        format!("{} line{}", n, if n == 1 { "" } else { "s" })
                    };
                    copy_to_clipboard(&text);
                    app.status_message = Some((format!("Copied {}", count), false));
                    app.clear_selection();
                }
            }
            _ => {}
        }
        return;
    }

    if app.show_config {
        handle_config_key(app, key);
        return;
    }

    if app.show_notifications {
        handle_notifications_key(app, key);
        return;
    }

    if app.show_bookmarks {
        handle_bookmarks_key(app, key);
        return;
    }

    match app.input_mode {
        InputMode::Idle => handle_idle_key(app, key),
        InputMode::Typing => handle_typing_key(app, key),
    }
}

fn handle_config_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.config_up(),
        KeyCode::Down => app.config_down(),
        KeyCode::Enter | KeyCode::Right => app.config_toggle(),
        KeyCode::Left => app.config_decrease(),
        KeyCode::Esc => {
            app.save_config();
            app.show_config = false;
            app.status_message = Some(("Settings saved".to_string(), false));
        }
        _ => {}
    }
}

fn handle_notifications_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.notification_up(),
        KeyCode::Down => app.notification_down(),
        KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
            app.notification_delete_selected()
        }
        KeyCode::Esc => app.show_notifications = false,
        _ => {}
    }
}

fn handle_bookmarks_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.bookmark_up(),
        KeyCode::Down => app.bookmark_down(),
        KeyCode::Enter => app.bookmark_select(),
        KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => app.bookmark_delete_selected(),
        KeyCode::Esc => app.show_bookmarks = false,
        _ => {}
    }
}

fn handle_idle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => app.scroll_up(1),
        KeyCode::Down => app.scroll_down(1),
        KeyCode::PageUp => app.scroll_up(app.viewport_height),
        KeyCode::PageDown => app.scroll_down(app.viewport_height),
        KeyCode::Home => app.scroll_to_top(),
        KeyCode::End => app.jump_to_bottom(),

        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                app.prev_match();
            } else if app.search.match_count() > 0 {
                app.next_match();
            }
        }

        KeyCode::Esc => {
            if app.show_help {
                app.show_help = false;
            } else if app.tail_view.is_some() {
                app.exit_tail_mode();
            } else if app.text_selection.is_some() || !app.selected_lines.is_empty() {
                app.clear_selection();
            } else if app.has_active_search() {
                app.search = crate::search::SearchState::new();
            } else if !app.filter_conditions.is_empty() || app.filtering {
                app.filter_conditions.clear();
                app.filter_highlight = None;
                app.filtered_lines.clear();
                app.filtering = false;
                app.scroll_offset = 0;
            } else if app.follow_mode {
                app.follow_mode = false;
                app.status_message = Some(("Follow mode OFF".to_string(), false));
            }
        }

        KeyCode::Char(c) => app.type_char(c),
        _ => {}
    }
}

fn handle_typing_key(app: &mut App, key: KeyEvent) {
    let suggestions_open = app.input.starts_with('/') && !app.command_suggestions.is_empty();

    match key.code {
        KeyCode::Esc => app.cancel_input(),
        KeyCode::Enter if suggestions_open && app.suggestion_index.is_some() => {
            app.accept_suggestion();
        }
        KeyCode::Enter => app.submit_input(),
        KeyCode::Backspace => app.input_backspace(),
        KeyCode::Delete => app.input_delete(),
        KeyCode::Left => app.input_left(),
        KeyCode::Right => app.input_right(),
        KeyCode::Home => app.input_home(),
        KeyCode::End => app.input_end(),
        KeyCode::Tab if suggestions_open => app.suggestion_next(),
        KeyCode::Tab => {}
        KeyCode::Up if suggestions_open => app.suggestion_prev(),
        KeyCode::Up => app.scroll_up(1),
        KeyCode::Down if suggestions_open => app.suggestion_next(),
        KeyCode::Down => app.scroll_down(1),
        KeyCode::PageUp => app.scroll_up(app.viewport_height),
        KeyCode::PageDown => app.scroll_down(app.viewport_height),
        KeyCode::Char(c) => app.type_char(c),
        _ => {}
    }
}

fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // macOS
    if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }

    // Linux (X11)
    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }

    // Linux (Wayland)
    if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}
