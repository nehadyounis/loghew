use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::colors;
use crate::app::{App, InputMode};

pub fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    if let Some((ref msg, is_error)) = app.status_message {
        if app.input_mode == InputMode::Idle {
            let style = if is_error {
                Style::default().fg(colors::INPUT_ERROR)
            } else {
                Style::default().fg(colors::INPUT_SUCCESS)
            };
            let msg_span = Span::styled(format!(" {}", msg), style);
            let w = msg_span.width();
            let mut msg_spans = vec![msg_span];
            if w < area.width as usize {
                msg_spans.push(Span::raw(" ".repeat(area.width as usize - w)));
            }
            f.render_widget(Paragraph::new(Line::from(msg_spans)).style(Style::reset()), area);
            return;
        }
    }

    let counts = app.source.index().level_counts();
    let current_line = app.actual_line(app.scroll_offset) + 1;
    let total = app.total_lines();

    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.filename),
            Style::default()
                .fg(colors::STATUS_FILENAME)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{} lines", format_number(total)),
            Style::default().fg(colors::STATUS_DIM),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("line {}", format_number(current_line)),
            Style::default().fg(colors::STATUS_FG),
        ),
    ];

    if app.follow_mode {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            "FOLLOWING",
            Style::default()
                .fg(colors::INPUT_SUCCESS)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if app.show_delta {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            "DELTA",
            Style::default().fg(colors::HINT_KEY),
        ));
    }


    if !app.filter_conditions.is_empty() {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            format!("FILTER ({})", format_number(app.filtered_lines.len())),
            Style::default().fg(app.config.warn_color),
        ));
    }

    if !app.bookmarks.is_empty() {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            format!("{} bookmarks", app.bookmarks.len()),
            Style::default().fg(colors::HINT_KEY),
        ));
    }

    if app.has_active_search() && app.input_mode == InputMode::Idle {
        if let Some(ref err) = app.search.error {
            spans.push(Span::styled(
                format!("   search error: {err}"),
                Style::default().fg(colors::INPUT_ERROR),
            ));
        } else {
            spans.push(Span::styled("   ", Style::default()));
            spans.push(Span::styled(
                format!("\"{}\"", app.search.pattern),
                Style::default().fg(colors::HINT_KEY),
            ));
            spans.push(Span::styled(
                format!(" {} matches", format_number(app.search.match_count())),
                Style::default().fg(if app.search.match_count() > 0 {
                    colors::INPUT_SUCCESS
                } else {
                    colors::INPUT_ERROR
                }),
            ));
        }
    }

    spans.push(Span::styled("   ", Style::default()));

    if counts.error > 0 {
        spans.push(Span::styled(
            format!("{} ", colors::DOT),
            Style::default().fg(app.config.error_color),
        ));
        spans.push(Span::styled(
            format!("{}  ", format_number(counts.error)),
            Style::default().fg(app.config.error_color),
        ));
    }
    if counts.warn > 0 {
        spans.push(Span::styled(
            format!("{} ", colors::DOT),
            Style::default().fg(app.config.warn_color),
        ));
        spans.push(Span::styled(
            format!("{}  ", format_number(counts.warn)),
            Style::default().fg(app.config.warn_color),
        ));
    }
    if counts.info > 0 {
        spans.push(Span::styled(
            format!("{} ", colors::DOT),
            Style::default().fg(app.config.info_color),
        ));
        spans.push(Span::styled(
            format!("{}  ", format_number(counts.info)),
            Style::default().fg(app.config.info_color),
        ));
    }
    if counts.debug > 0 {
        spans.push(Span::styled(
            format!("{} ", colors::DOT),
            Style::default().fg(app.config.debug_color),
        ));
        spans.push(Span::styled(
            format!("{}  ", format_number(counts.debug)),
            Style::default().fg(app.config.debug_color),
        ));
    }

    let w: usize = spans.iter().map(|s| s.width()).sum();
    if w < area.width as usize {
        spans.push(Span::raw(" ".repeat(area.width as usize - w)));
    }
    let line = Line::from(spans);
    let bar = Paragraph::new(line).style(Style::reset());
    f.render_widget(bar, area);
}

pub fn draw_hints(f: &mut Frame, app: &App, area: Rect) {
    let key_style = Style::default().fg(colors::HINT_KEY);
    let text_style = Style::default().fg(colors::HINT_TEXT);
    let sep = Span::styled("  ", text_style);

    let mut spans = match app.input_mode {
        InputMode::Idle => {
            if app.show_help {
                vec![
                    Span::styled(" Esc", key_style),
                    Span::styled(" close help", text_style),
                ]
            } else if app.has_active_search() && app.search.match_count() > 0 {
                vec![
                    Span::styled(" Enter", key_style),
                    Span::styled(" next", text_style),
                    sep.clone(),
                    Span::styled("Shift+Enter", key_style),
                    Span::styled(" prev", text_style),
                    sep.clone(),
                    Span::styled("Esc", key_style),
                    Span::styled(" clear", text_style),
                ]
            } else if !app.filter_conditions.is_empty() {
                vec![
                    Span::styled(" Esc", key_style),
                    Span::styled(" clear filter", text_style),
                    Span::styled("  ·  ", text_style),
                    Span::styled("Type to search", text_style),
                    Span::styled("  ·  ", text_style),
                    Span::styled("/", key_style),
                    Span::styled(" for commands", text_style),
                ]
            } else {
                vec![
                    Span::styled(" Type to search", text_style),
                    Span::styled("  ·  ", text_style),
                    Span::styled("/", key_style),
                    Span::styled(" for commands", text_style),
                    Span::styled("  ·  ", text_style),
                    Span::styled("/q", key_style),
                    Span::styled(" quit", text_style),
                ]
            }
        }
        InputMode::Typing => {
            if app.input.starts_with('/') {
                vec![
                    Span::styled(" Tab", key_style),
                    Span::styled(" complete", text_style),
                    sep.clone(),
                    Span::styled("Enter", key_style),
                    Span::styled(" run", text_style),
                    sep.clone(),
                    Span::styled("Esc", key_style),
                    Span::styled(" cancel", text_style),
                ]
            } else {
                vec![
                    Span::styled(" Enter", key_style),
                    Span::styled(" search", text_style),
                    sep.clone(),
                    Span::styled("Esc", key_style),
                    Span::styled(" cancel", text_style),
                ]
            }
        }
    };

    let w: usize = spans.iter().map(|s| s.width()).sum();
    if w < area.width as usize {
        spans.push(Span::raw(" ".repeat(area.width as usize - w)));
    }
    let hint = Paragraph::new(Line::from(spans)).style(Style::reset());
    f.render_widget(hint, area);
}

fn format_number(n: usize) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
