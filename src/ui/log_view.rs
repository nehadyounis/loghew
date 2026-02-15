use std::borrow::Cow;

use regex::Regex;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::colors;
use crate::app::{App, InputMode};
use crate::log::LogLevel;

fn sanitize_line(text: &str) -> Cow<'_, str> {
    if !text.bytes().any(|b| b < 0x20 || b == 0x7F) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.as_str().starts_with('[') {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c == '\t' {
            out.push_str("    ");
        } else if !c.is_control() {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let total = app.total_lines();
    let gutter_width = if total == 0 {
        1
    } else {
        (total as f64).log10().floor() as usize + 1
    }
    .max(1);

    let highlight_re = app.highlight_regex();

    // Pre-compute sanitized line texts so they live long enough for span borrows
    let sanitized: Vec<Cow<str>> = (0..area.height as usize)
        .map(|i| {
            let visible_idx = app.scroll_offset + i;
            if visible_idx >= app.visible_count() {
                Cow::Borrowed("")
            } else {
                let line_num = app.actual_line(visible_idx);
                sanitize_line(app.source.get_line(line_num).unwrap_or(""))
            }
        })
        .collect();

    let mut lines = Vec::with_capacity(area.height as usize);

    for i in 0..area.height as usize {
        let visible_idx = app.scroll_offset + i;
        if visible_idx >= app.visible_count() {
            lines.push(Line::from(Span::raw(" ".repeat(area.width as usize))));
            continue;
        }

        let line_num = app.actual_line(visible_idx);
        let line_text: &str = &sanitized[i];
        let level = app
            .source
            .index()
            .levels
            .get(line_num)
            .copied()
            .unwrap_or(LogLevel::Unknown);

        let bookmark_marker = if app.is_bookmarked(line_num) {
            Span::styled("◆", Style::default().fg(colors::HINT_KEY))
        } else {
            Span::styled(" ", Style::default())
        };

        let gutter = Span::styled(
            format!("{:>width$}", line_num + 1, width = gutter_width),
            Style::default().fg(colors::LINE_NUM),
        );

        let separator = Span::styled(" │ ", Style::default().fg(colors::GUTTER_SEP));

        let text_style = level_text_style(level, app);

        let delta_span = if app.show_delta {
            let index = app.source.index();
            let delta_text = if line_num == 0 {
                "+0ms".to_string()
            } else {
                let cur_ts = index.timestamps.get(line_num).and_then(|t| *t);
                let prev_ts = index.timestamps.get(line_num - 1).and_then(|t| *t);
                match (cur_ts, prev_ts) {
                    (Some(cur), Some(prev)) => format_delta(cur - prev),
                    _ => "     ".to_string(),
                }
            };
            let delta_style = if app.show_delta {
                delta_color(&delta_text, app)
            } else {
                Style::default().fg(colors::STATUS_DIM)
            };
            Some(Span::styled(format!("{:<9}", delta_text), delta_style))
        } else {
            None
        };

        let is_current = app.input_mode == InputMode::Idle
            && app.search.is_current_match_line(line_num);

        let content_spans = if let Some((sc, ec)) = app.line_text_selection(line_num) {
            build_selected_spans(line_text, sc, ec, text_style)
        } else {
            match highlight_re {
                Some(re) => highlight_matches(line_text, re, is_current, text_style),
                None => vec![Span::styled(line_text, text_style)],
            }
        };

        let is_selected = app.selected_lines.contains(&line_num);

        let mut spans = vec![bookmark_marker];
        if app.config.line_numbers {
            spans.push(gutter);
        }
        spans.push(separator);
        if let Some(delta) = delta_span {
            spans.push(delta);
        }
        spans.extend(content_spans);

        let total_width: usize = spans.iter().map(|s| s.width()).sum();
        if total_width < area.width as usize {
            spans.push(Span::raw(" ".repeat(area.width as usize - total_width)));
        }

        let line = if is_selected {
            Line::from(spans).style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(spans)
        };
        lines.push(line);
    }

    let paragraph = Paragraph::new(lines).style(Style::reset());
    f.render_widget(paragraph, area);
}

fn format_delta(ms: i64) -> String {
    let ms = ms.abs();
    if ms < 1000 {
        format!("+{}ms", ms)
    } else if ms < 60_000 {
        format!("+{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("+{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("+{:.1}h", ms as f64 / 3_600_000.0)
    }
}

fn delta_color(delta: &str, app: &App) -> Style {
    if delta.contains("ms") {
        let num: f64 = delta
            .trim_start_matches('+')
            .trim_end_matches("ms")
            .parse()
            .unwrap_or(0.0);
        if num < 100.0 {
            Style::default().fg(app.config.debug_color)
        } else {
            Style::default().fg(colors::STATUS_FG)
        }
    } else if delta.contains('h') {
        Style::default()
            .fg(app.config.error_color)
            .add_modifier(Modifier::BOLD)
    } else if delta.contains('m') {
        Style::default().fg(app.config.warn_color)
    } else if delta.contains('s') {
        let num: f64 = delta
            .trim_start_matches('+')
            .trim_end_matches('s')
            .parse()
            .unwrap_or(0.0);
        if num > 10.0 {
            Style::default()
                .fg(app.config.error_color)
                .add_modifier(Modifier::BOLD)
        } else if num > 1.0 {
            Style::default().fg(app.config.warn_color)
        } else {
            Style::default().fg(colors::STATUS_FG)
        }
    } else {
        Style::default().fg(colors::STATUS_DIM)
    }
}

fn level_text_style(level: LogLevel, app: &App) -> Style {
    match level {
        LogLevel::Error => Style::default()
            .fg(app.config.error_color)
            .add_modifier(Modifier::BOLD),
        LogLevel::Warn => Style::default().fg(app.config.warn_color),
        LogLevel::Info => Style::default().fg(app.config.info_color),
        LogLevel::Debug => Style::default().fg(app.config.debug_color),
        LogLevel::Trace => Style::default().fg(app.config.trace_color),
        LogLevel::Unknown => Style::default().fg(app.config.info_color),
    }
}

fn build_selected_spans<'a>(
    line: &'a str,
    start_char: usize,
    end_char: usize,
    base_style: Style,
) -> Vec<Span<'a>> {
    let sel_style = Style::default()
        .fg(colors::INPUT_TEXT)
        .add_modifier(Modifier::REVERSED);
    let mut spans = Vec::new();

    let sc_byte = line
        .char_indices()
        .nth(start_char)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    let ec_byte = line
        .char_indices()
        .nth(end_char)
        .map(|(i, _)| i)
        .unwrap_or(line.len());

    if sc_byte > 0 {
        spans.push(Span::styled(&line[..sc_byte], base_style));
    }
    if sc_byte < ec_byte {
        spans.push(Span::styled(&line[sc_byte..ec_byte], sel_style));
    }
    if ec_byte < line.len() {
        spans.push(Span::styled(&line[ec_byte..], base_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(line, base_style));
    }
    spans
}

fn highlight_matches<'a>(
    line: &'a str,
    re: &Regex,
    is_current: bool,
    base_style: Style,
) -> Vec<Span<'a>> {
    let highlight_style = if is_current {
        base_style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        base_style.add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
    };

    let mut spans = Vec::new();
    let mut last_end = 0;

    for m in re.find_iter(line) {
        if m.start() > last_end {
            spans.push(Span::styled(&line[last_end..m.start()], base_style));
        }
        spans.push(Span::styled(m.as_str(), highlight_style));
        last_end = m.end();
    }

    if last_end < line.len() {
        spans.push(Span::styled(&line[last_end..], base_style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(line, base_style));
    }

    spans
}
