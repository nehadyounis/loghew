use std::borrow::Cow;

use regex::Regex;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::colors;
use crate::app::{App, InputMode, TailView};
use crate::log::{LogLevel, LEVEL_REGEX};

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

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if let Some(ref tail) = app.tail_view {
        draw_tail(f, app, area, tail);
        return;
    }

    let total = app.total_lines();
    let gutter_width = if total == 0 {
        1
    } else {
        (total as f64).log10().floor() as usize + 1
    }
    .max(1);

    let highlight_re = app.highlight_regex().cloned();

    if app.wrap_mode {
        draw_wrapped(f, app, area, gutter_width, highlight_re.as_ref());
        return;
    }

    app.wrap_row_map.clear();
    app.wrap_char_offsets.clear();

    let highlight_re = highlight_re.as_ref();

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
        } else if app.semantic_color {
            let spans = tokenize_semantic(line_text, app);
            match highlight_re {
                Some(re) => overlay_highlights(spans, re, line_text, is_current),
                None => spans,
            }
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

fn draw_wrapped(f: &mut Frame, app: &mut App, area: Rect, gutter_width: usize, highlight_re: Option<&Regex>) {
    let height = area.height as usize;
    let prefix_width = 1
        + if app.config.line_numbers { gutter_width } else { 0 }
        + 3
        + if app.show_delta { 9 } else { 0 };
    let content_width = (area.width as usize).saturating_sub(prefix_width).max(1);

    // Phase 1: split each log line into character-width chunks
    struct ChunkInfo {
        visible_idx: usize,
        text: String,
        is_first: bool,
        char_offset: usize,
    }
    let mut chunks: Vec<ChunkInfo> = Vec::with_capacity(height);
    let mut log_idx = 0;
    while chunks.len() < height {
        let visible_idx = app.scroll_offset + log_idx;
        if visible_idx >= app.visible_count() {
            break;
        }
        let line_num = app.actual_line(visible_idx);
        let raw = sanitize_line(app.source.get_line(line_num).unwrap_or(""));
        if raw.is_empty() {
            chunks.push(ChunkInfo { visible_idx, text: String::new(), is_first: true, char_offset: 0 });
        } else {
            let chars: Vec<char> = raw.chars().collect();
            let mut pos = 0;
            let mut first = true;
            while pos < chars.len() && chunks.len() < height {
                let end = (pos + content_width).min(chars.len());
                chunks.push(ChunkInfo {
                    visible_idx,
                    text: chars[pos..end].iter().collect(),
                    is_first: first,
                    char_offset: pos,
                });
                first = false;
                pos = end;
            }
        }
        log_idx += 1;
    }

    // Populate wrap_row_map and wrap_char_offsets so click/drag resolve correctly
    app.wrap_row_map.clear();
    app.wrap_char_offsets.clear();
    for chunk in &chunks {
        app.wrap_row_map.push(chunk.visible_idx);
        app.wrap_char_offsets.push(chunk.char_offset);
    }
    for _ in chunks.len()..height {
        app.wrap_row_map.push(app.scroll_offset);
        app.wrap_char_offsets.push(0);
    }

    // Phase 2: build styled Lines from the pre-computed chunks
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for ci in 0..chunks.len() {
        let chunk = &chunks[ci];
        let line_num = app.actual_line(chunk.visible_idx);
        let level = app.source.index().levels.get(line_num).copied().unwrap_or(LogLevel::Unknown);
        let text_style = level_text_style(level, app);
        let is_selected = app.selected_lines.contains(&line_num);
        let is_current = app.input_mode == InputMode::Idle && app.search.is_current_match_line(line_num);

        let chunk_char_len = chunk.text.chars().count();
        let content_spans = if let Some((sc, ec)) = app.line_text_selection(line_num) {
            // Map line-level char range to this chunk's range
            let chunk_start = chunk.char_offset;
            let chunk_end = chunk.char_offset + chunk_char_len;
            let sel_start = sc.max(chunk_start).saturating_sub(chunk_start);
            let sel_end = ec.min(chunk_end).saturating_sub(chunk_start);
            if sel_start < sel_end {
                build_selected_spans(&chunk.text, sel_start, sel_end, text_style)
            } else {
                vec![Span::styled(chunk.text.as_str(), text_style)]
            }
        } else if app.semantic_color {
            let spans = tokenize_semantic(&chunk.text, app);
            match highlight_re {
                Some(re) => overlay_highlights(spans, re, &chunk.text, is_current),
                None => spans,
            }
        } else {
            match highlight_re {
                Some(re) => highlight_matches(&chunk.text, re, is_current, text_style),
                None => vec![Span::styled(chunk.text.as_str(), text_style)],
            }
        };

        let mut spans: Vec<Span> = Vec::new();
        if chunk.is_first {
            spans.push(if app.is_bookmarked(line_num) {
                Span::styled("◆", Style::default().fg(colors::HINT_KEY))
            } else {
                Span::styled(" ", Style::default())
            });
            if app.config.line_numbers {
                spans.push(Span::styled(
                    format!("{:>width$}", line_num + 1, width = gutter_width),
                    Style::default().fg(colors::LINE_NUM),
                ));
            }
            spans.push(Span::styled(" │ ", Style::default().fg(colors::GUTTER_SEP)));
            if app.show_delta {
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
                let delta_style = delta_color(&delta_text, app);
                spans.push(Span::styled(format!("{:<9}", delta_text), delta_style));
            }
        } else {
            // Continuation: keep the separator continuous
            spans.push(Span::styled(" ", Style::default()));
            if app.config.line_numbers {
                spans.push(Span::raw(" ".repeat(gutter_width)));
            }
            spans.push(Span::styled(" │ ", Style::default().fg(colors::GUTTER_SEP)));
            if app.show_delta {
                spans.push(Span::raw(" ".repeat(9)));
            }
        }
        spans.extend(content_spans);

        let line = if is_selected {
            Line::from(spans).style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(spans)
        };
        lines.push(line);
    }

    while lines.len() < height {
        lines.push(Line::from(Span::raw(" ".repeat(area.width as usize))));
    }

    let paragraph = Paragraph::new(lines).style(Style::reset());
    f.render_widget(paragraph, area);
}

fn draw_tail(f: &mut Frame, app: &App, area: Rect, tail: &TailView) {
    let height = area.height as usize;
    let total = tail.lines.len();
    let start = total.saturating_sub(height);

    let normal_gutter = if app.total_lines() == 0 { 1 } else {
        (app.total_lines() as f64).log10().floor() as usize + 1
    }.max(1);

    let sanitized: Vec<Cow<str>> = (0..height)
        .map(|i| {
            let idx = start + i;
            if idx >= total {
                Cow::Borrowed("")
            } else {
                sanitize_line(&tail.lines[idx])
            }
        })
        .collect();

    let mut lines = Vec::with_capacity(height);

    for i in 0..height {
        let idx = start + i;
        if idx >= total {
            lines.push(Line::from(Span::raw(" ".repeat(area.width as usize))));
            continue;
        }

        let level = tail.levels[idx];
        let text_style = level_text_style(level, app);
        let line_text: &str = &sanitized[i];

        let mut spans: Vec<Span> = vec![Span::styled(" ", Style::default())];

        if app.config.line_numbers {
            spans.push(Span::styled(
                format!("{:>width$}", "~", width = normal_gutter),
                Style::default().fg(colors::LINE_NUM),
            ));
        }

        spans.push(Span::styled(" │ ", Style::default().fg(colors::GUTTER_SEP)));

        if app.show_delta {
            spans.push(Span::raw(" ".repeat(9)));
        }

        if app.semantic_color {
            spans.extend(tokenize_semantic(line_text, app));
        } else {
            spans.push(Span::styled(line_text, text_style));
        }

        let total_width: usize = spans.iter().map(|s| s.width()).sum();
        if total_width < area.width as usize {
            spans.push(Span::raw(" ".repeat(area.width as usize - total_width)));
        }

        lines.push(Line::from(spans));
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

fn tokenize_semantic<'a>(line: &'a str, app: &App) -> Vec<Span<'a>> {
    if line.is_empty() {
        return vec![Span::styled(line, Style::default().fg(colors::SEMANTIC_TEXT))];
    }

    // Collect (start, end, style) segments, then fill gaps with default text
    let mut segments: Vec<(usize, usize, Style)> = Vec::new();

    // 1. Timestamp (extend past trailing .NNN or ,NNN milliseconds)
    let index = app.source.index();
    if let Some(ref ts_fmt) = index.timestamp_format {
        if let Some(m) = ts_fmt.regex.find(line) {
            let mut end = m.end();
            let bytes = line.as_bytes();
            if end < bytes.len() && (bytes[end] == b'.' || bytes[end] == b',') {
                let mut j = end + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                if j > end + 1 {
                    end = j;
                }
            }
            // Also grab trailing timezone like Z or +00:00
            if end < bytes.len() && bytes[end] == b'Z' {
                end += 1;
            } else if end + 5 < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                if bytes[end + 1].is_ascii_digit() {
                    end += 6; // +00:00
                    end = end.min(bytes.len());
                }
            }
            segments.push((m.start(), end, Style::default().fg(colors::SEMANTIC_TIMESTAMP)));
        }
    }

    // 2. Level keyword
    if let Some(m) = LEVEL_REGEX.find(line) {
        let level_style = match m.as_str().to_ascii_uppercase().as_str() {
            "ERROR" | "FATAL" | "CRITICAL" => Style::default().fg(app.config.error_color).add_modifier(Modifier::BOLD),
            "WARN" | "WARNING" => Style::default().fg(app.config.warn_color).add_modifier(Modifier::BOLD),
            "INFO" => Style::default().fg(app.config.info_color).add_modifier(Modifier::BOLD),
            "DEBUG" => Style::default().fg(app.config.debug_color).add_modifier(Modifier::BOLD),
            "TRACE" => Style::default().fg(app.config.trace_color).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(colors::SEMANTIC_TEXT),
        };
        segments.push((m.start(), m.end(), level_style));
    }

    // Sort segments by start position
    segments.sort_by_key(|(s, _, _)| *s);

    // 3. Scan remaining text for brackets, strings, and numbers
    let covered = segments.clone();
    let text_style = Style::default().fg(colors::SEMANTIC_TEXT);
    let mut extra: Vec<(usize, usize, Style)> = Vec::new();

    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip if inside an already-covered segment
        if covered.iter().any(|(s, e, _)| i >= *s && i < *e) {
            i += 1;
            continue;
        }

        match bytes[i] {
            b'[' => {
                if let Some(close) = line[i..].find(']') {
                    let end = i + close + 1;
                    let bracket_style = Style::default().fg(colors::SEMANTIC_BRACKET);
                    // Split bracket around any covered segments (e.g. level keywords)
                    let mut pos = i;
                    for &(cs, ce, _) in &covered {
                        if cs >= end || ce <= i {
                            continue;
                        }
                        if cs > pos {
                            extra.push((pos, cs, bracket_style));
                        }
                        pos = ce;
                    }
                    if pos < end {
                        extra.push((pos, end, bracket_style));
                    }
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                let closer = if quote == b'"' { '"' } else { '\'' };
                if let Some(close) = line[i + 1..].find(closer) {
                    let end = i + 1 + close + 1;
                    let content = &line[i..end];
                    let style = if end < line.len() && bytes.get(end) == Some(&b':') {
                        Style::default().fg(colors::SEMANTIC_KEY)
                    } else if content.contains(':') && content.len() < 40 {
                        Style::default().fg(colors::SEMANTIC_KEY)
                    } else {
                        Style::default().fg(colors::SEMANTIC_STRING)
                    };
                    extra.push((i, end, style));
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                // Check word boundary before: must be start of string or non-alphanumeric
                if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                // Check word boundary after
                if i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    continue;
                }
                extra.push((start, i, Style::default().fg(colors::SEMANTIC_NUMBER)));
            }
            _ => {
                i += 1;
            }
        }
    }

    segments.extend(extra);
    segments.sort_by_key(|(s, _, _)| *s);

    // Build final spans, filling gaps with default text
    let mut spans = Vec::new();
    let mut pos = 0;
    for &(start, end, style) in &segments {
        if start > pos {
            spans.push(Span::styled(&line[pos..start], text_style));
        }
        if start >= pos {
            spans.push(Span::styled(&line[start..end], style));
            pos = end;
        }
    }
    if pos < line.len() {
        spans.push(Span::styled(&line[pos..], text_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(line, text_style));
    }
    spans
}

fn overlay_highlights<'a>(
    spans: Vec<Span<'a>>,
    re: &Regex,
    full_text: &'a str,
    is_current: bool,
) -> Vec<Span<'a>> {
    let highlight_mod = if is_current {
        Modifier::REVERSED | Modifier::BOLD
    } else {
        Modifier::UNDERLINED | Modifier::BOLD
    };

    let matches: Vec<(usize, usize)> = re.find_iter(full_text).map(|m| (m.start(), m.end())).collect();
    if matches.is_empty() {
        return spans;
    }

    // Flatten spans into (byte_start, byte_end, style) using full_text offsets
    let mut styled_ranges: Vec<(usize, usize, Style)> = Vec::new();
    let mut offset = 0;
    for span in &spans {
        let len = span.content.len();
        styled_ranges.push((offset, offset + len, span.style));
        offset += len;
    }

    let mut result = Vec::new();
    for (range_start, range_end, base_style) in styled_ranges {
        let mut cuts: Vec<(usize, usize)> = Vec::new();
        for &(ms, me) in &matches {
            let os = ms.max(range_start);
            let oe = me.min(range_end);
            if os < oe {
                cuts.push((os, oe));
            }
        }

        if cuts.is_empty() {
            result.push(Span::styled(&full_text[range_start..range_end], base_style));
        } else {
            let mut pos = range_start;
            for (cs, ce) in cuts {
                if cs > pos {
                    result.push(Span::styled(&full_text[pos..cs], base_style));
                }
                result.push(Span::styled(
                    &full_text[cs..ce],
                    base_style.add_modifier(highlight_mod),
                ));
                pos = ce;
            }
            if pos < range_end {
                result.push(Span::styled(&full_text[pos..range_end], base_style));
            }
        }
    }

    result
}
