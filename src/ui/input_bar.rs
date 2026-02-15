use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::colors;
use crate::app::{App, SLASH_COMMANDS};

pub fn draw_input_line(f: &mut Frame, app: &App, area: Rect) {
    let prompt = " ❯ ";
    let prompt_style = Style::default()
        .fg(colors::INPUT_PROMPT)
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![Span::styled(prompt, prompt_style)];

    if app.input.is_empty() {
        spans.push(Span::styled(
            "type to search · / for commands",
            Style::default().fg(colors::INPUT_PLACEHOLDER),
        ));
    } else if app.input.starts_with('/') {
        let cmd_style = Style::default()
            .fg(colors::INPUT_PROMPT)
            .add_modifier(Modifier::BOLD);
        let arg_style = Style::default().fg(colors::INPUT_TEXT);
        if let Some(space_pos) = app.input.find(' ') {
            spans.push(Span::styled(app.input[..space_pos].to_string(), cmd_style));
            spans.push(Span::styled(app.input[space_pos..].to_string(), arg_style));
        } else {
            spans.push(Span::styled(app.input.clone(), cmd_style));
        }
    } else {
        spans.push(Span::styled(
            app.input.clone(),
            Style::default().fg(colors::INPUT_TEXT),
        ));
    }

    let w: usize = spans.iter().map(|s| s.width()).sum();
    if w < area.width as usize {
        spans.push(Span::raw(" ".repeat(area.width as usize - w)));
    }
    let input_line = Paragraph::new(Line::from(spans)).style(Style::reset());
    f.render_widget(input_line, area);

    let prompt_width = UnicodeWidthStr::width(prompt);
    let input_before_cursor = &app.input[..app.input_cursor];
    let input_width = UnicodeWidthStr::width(input_before_cursor);
    let cursor_x = area.x + prompt_width as u16 + input_width as u16;
    f.set_cursor_position((cursor_x, area.y));
}

pub fn draw_suggestions_inline(f: &mut Frame, app: &App, area: Rect) {
    let count = area.height as usize;
    if count == 0 {
        return;
    }

    let mut lines = Vec::new();
    for (i, &cmd_idx) in app.command_suggestions.iter().take(count).enumerate() {
        let cmd = &SLASH_COMMANDS[cmd_idx];
        let is_selected = app.suggestion_index == Some(i);

        let (marker, name_style, desc_style) = if is_selected {
            (
                Span::styled(" ▸ ", Style::default().fg(colors::HINT_KEY)),
                Style::default()
                    .fg(colors::HINT_KEY)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(colors::INPUT_TEXT),
            )
        } else {
            (
                Span::styled("   ", Style::default()),
                Style::default().fg(colors::POPUP_CMD),
                Style::default().fg(colors::POPUP_DESC),
            )
        };

        let padded_name = format!("/{:<14}", cmd.name);
        let desc = cmd.description.to_string();

        let mut sug_spans = vec![
            marker,
            Span::styled(padded_name, name_style),
            Span::styled(desc, desc_style),
        ];
        let sw: usize = sug_spans.iter().map(|s| s.width()).sum();
        if sw < area.width as usize {
            sug_spans.push(Span::raw(" ".repeat(area.width as usize - sw)));
        }
        lines.push(Line::from(sug_spans));
    }

    let suggestions = Paragraph::new(lines).style(Style::reset());
    f.render_widget(suggestions, area);
}
