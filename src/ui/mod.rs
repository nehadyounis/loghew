pub mod colors;
mod input_bar;
mod log_view;
mod status_bar;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::app::{App, InputMode};

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Clear, f.area());

    let has_suggestions = app.input_mode == InputMode::Typing
        && app.input.starts_with('/')
        && !app.command_suggestions.is_empty();

    let suggestion_count = if has_suggestions {
        app.command_suggestions.len().min(10) as u16
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                      // log view / panel
            Constraint::Length(1),                    // separator
            Constraint::Length(suggestion_count),     // suggestions (0 when hidden)
            Constraint::Length(1),                    // input bar
            Constraint::Length(1),                    // separator
            Constraint::Length(1),                    // status line
            Constraint::Length(1),                    // hint line
        ])
        .split(f.area());

    app.viewport_height = chunks[0].height as usize;

    if app.show_help {
        draw_help(f, chunks[0]);
    } else if app.show_config {
        draw_config(f, app, chunks[0]);
    } else if app.show_notifications {
        draw_notifications(f, app, chunks[0]);
    } else if app.show_bookmarks {
        draw_bookmarks(f, app, chunks[0]);
    } else {
        log_view::draw(f, app, chunks[0]);
    }

    draw_separator(f, chunks[1]);

    if has_suggestions {
        input_bar::draw_suggestions_inline(f, app, chunks[2]);
    }

    input_bar::draw_input_line(f, app, chunks[3]);
    draw_separator(f, chunks[4]);
    status_bar::draw_status(f, app, chunks[5]);
    status_bar::draw_hints(f, app, chunks[6]);
}

fn draw_separator(f: &mut Frame, area: Rect) {
    let line = "─".repeat(area.width as usize);
    let sep = Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(colors::GUTTER_SEP),
    )))
    .style(Style::reset());
    f.render_widget(sep, area);
}

fn draw_config(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<Line> = App::CONFIG_ITEMS
        .iter()
        .enumerate()
        .map(|(i, &label)| {
            let is_selected = i == app.config_cursor;
            let value = app.config_value(i);
            let prefix = if is_selected { "  ▸ " } else { "    " };

            let label_style = if is_selected {
                Style::default()
                    .fg(colors::INPUT_TEXT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::POPUP_DESC)
            };

            let val_style = if value == "ON" {
                Style::default()
                    .fg(colors::INPUT_SUCCESS)
                    .add_modifier(Modifier::BOLD)
            } else if value == "OFF" {
                Style::default().fg(colors::STATUS_DIM)
            } else {
                Style::default()
                    .fg(colors::HINT_KEY)
                    .add_modifier(Modifier::BOLD)
            };

            let arrow_style = Style::default().fg(colors::STATUS_DIM);

            let mut spans = vec![
                Span::styled(prefix, label_style),
                Span::styled(format!("{:<16}", label), label_style),
            ];
            if i == 0 || i == 3 {
                spans.push(Span::styled("◀ ", arrow_style));
                spans.push(Span::styled(value.clone(), val_style));
                spans.push(Span::styled(" ▶", arrow_style));
            } else {
                spans.push(Span::styled(value, val_style));
            }
            Line::from(spans)
        })
        .collect();

    let hint = Line::from(vec![
        Span::styled("  ↑↓", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" navigate  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("←→", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" change  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("Esc", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" save", Style::default().fg(colors::HINT_TEXT)),
    ]);

    let mut all_lines = vec![Line::from("")];
    all_lines.extend(items);
    all_lines.push(Line::from(""));
    all_lines.push(hint);

    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::HINT_KEY));

    pad_panel(area, &mut all_lines);
    let panel = Paragraph::new(all_lines).block(block).style(Style::reset());
    f.render_widget(panel, area);
}

fn draw_notifications(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<Line> = app
        .notify_entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == app.notification_cursor;
            let prefix = if is_selected { "  ▸ " } else { "    " };
            let style = if is_selected {
                Style::default()
                    .fg(colors::INPUT_TEXT)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(colors::POPUP_DESC)
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&entry.pattern, style),
            ])
        })
        .collect();

    let hint = Line::from(vec![
        Span::styled("  ↑↓", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" navigate  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("d", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" remove  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("Esc", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" close", Style::default().fg(colors::HINT_TEXT)),
    ]);

    let mut all_lines = vec![Line::from("")];
    all_lines.extend(items);
    all_lines.push(Line::from(""));
    all_lines.push(hint);

    let block = Block::default()
        .title(" Notifications ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::HINT_KEY));

    pad_panel(area, &mut all_lines);
    let panel = Paragraph::new(all_lines).block(block).style(Style::reset());
    f.render_widget(panel, area);
}

fn draw_bookmarks(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<Line> = app
        .bookmarks
        .iter()
        .enumerate()
        .map(|(i, (line_num, label))| {
            let is_selected = i == app.bookmark_cursor;
            let prefix = if is_selected { "  ▸ " } else { "    " };
            let style = if is_selected {
                Style::default()
                    .fg(colors::INPUT_TEXT)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(colors::POPUP_DESC)
            };
            let line_style = if is_selected {
                style
            } else {
                Style::default().fg(colors::HINT_KEY)
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("L{:<6}", line_num + 1), line_style),
                Span::styled(label.clone(), style),
            ])
        })
        .collect();

    let hint = Line::from(vec![
        Span::styled("  ↑↓", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" navigate  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("Enter", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" jump  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("d", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" delete  ", Style::default().fg(colors::HINT_TEXT)),
        Span::styled("Esc", Style::default().fg(colors::HINT_KEY)),
        Span::styled(" close", Style::default().fg(colors::HINT_TEXT)),
    ]);

    let mut all_lines = vec![Line::from("")];
    all_lines.extend(items);
    all_lines.push(Line::from(""));
    all_lines.push(hint);

    let block = Block::default()
        .title(" Bookmarks ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::HINT_KEY));

    pad_panel(area, &mut all_lines);
    let panel = Paragraph::new(all_lines).block(block).style(Style::reset());
    f.render_widget(panel, area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let help_lines = vec![
        "",
        "  Navigation",
        "  Up/Down         Scroll line",
        "  PgUp/PgDn       Scroll page",
        "  Home/End        Top / bottom",
        "  Mouse wheel     Scroll",
        "  Mouse click     Select line",
        "",
        "  Search",
        "  <type text>     Search (literal, case-insensitive)",
        "  Enter           Next match",
        "  Shift+Enter     Previous match",
        "  Esc             Clear search",
        "",
        "  Commands",
        "  /regex <pat>    Regex search",
        "  /only-show <p>  Show only matching lines",
        "  /time <t>       Jump to timestamp (HH:MM, -5m, +1h)",
        "  /go <n|name>    Go to line number or bookmark",
        "  /bookmark [n]   Toggle bookmark on current line",
        "  /bookmarks      Open bookmark list",
        "  /notify <word>  Add desktop notification",
        "  /notifications  Manage notifications",
        "  /follow         Toggle auto-scroll to bottom",
        "  /delta          Toggle time delta display",
        "  /top            Go to first line",
        "  /bottom /end    Go to last line",
        "  /config         Open settings",
        "  /help           Toggle this help",
        "  /quit /exit     Quit",
        "",
        "  Press Esc or /help to close",
        "",
    ];

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::HINT_KEY));

    let mut text: Vec<Line> = help_lines
        .iter()
        .map(|l| {
            if l.is_empty() {
                Line::from("")
            } else if l.starts_with("  /") {
                let trimmed = l.trim_start();
                if let Some(space_pos) = trimmed.find("  ") {
                    let cmd = &trimmed[..space_pos];
                    let desc = trimmed[space_pos..].trim_start();
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("{:<18}", cmd),
                            Style::default()
                                .fg(colors::HINT_KEY)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            desc.to_string(),
                            Style::default().fg(colors::POPUP_DESC),
                        ),
                    ])
                } else {
                    Line::from(Span::styled(
                        l.to_string(),
                        Style::default().fg(colors::HINT_KEY),
                    ))
                }
            } else if l.trim_start().starts_with('<')
                || l.contains("Scroll")
                || l.contains("Enter")
                || l.contains("Esc")
                || l.contains("Mouse")
                || l.contains("Shift")
            {
                let trimmed = l.trim_start();
                if let Some(space_pos) = trimmed.find("  ") {
                    let key = &trimmed[..space_pos];
                    let desc = trimmed[space_pos..].trim_start();
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("{:<18}", key),
                            Style::default().fg(colors::POPUP_CMD),
                        ),
                        Span::styled(
                            desc.to_string(),
                            Style::default().fg(colors::POPUP_DESC),
                        ),
                    ])
                } else {
                    Line::from(Span::styled(
                        l.to_string(),
                        Style::default().fg(colors::POPUP_CMD),
                    ))
                }
            } else if l.contains("Navigation") || l.contains("Search") || l.contains("Commands") {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default()
                        .fg(colors::STATUS_FILENAME)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if l.contains("Press Esc") {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(colors::HINT_TEXT),
                ))
            } else {
                Line::from(Span::styled(
                    l.to_string(),
                    Style::default().fg(colors::POPUP_DESC),
                ))
            }
        })
        .collect();

    pad_panel(area, &mut text);
    let panel = Paragraph::new(text).block(block).style(Style::reset());
    f.render_widget(panel, area);
}

fn pad_panel(area: Rect, lines: &mut Vec<Line<'_>>) {
    let w = area.width.saturating_sub(2) as usize;
    let h = area.height.saturating_sub(2) as usize;
    for line in lines.iter_mut() {
        let lw = line.width();
        if lw < w {
            line.spans.push(Span::raw(" ".repeat(w - lw)));
        }
    }
    while lines.len() < h {
        lines.push(Line::from(" ".repeat(w)));
    }
}
