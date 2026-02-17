use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use regex::Regex;

use crate::config::{self, Config};
use crate::log::{LogLevel, LogSource};
use crate::search::SearchState;
use crate::worker::{self, Generation, WorkRequest, WorkResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Idle,
    Typing,
}

#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub has_arg: bool,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "regex", description: "Regex search  /r error.*timeout", has_arg: true },
    SlashCommand { name: "follow", description: "Toggle auto-scroll to bottom  /f", has_arg: false },
    SlashCommand { name: "filter", description: "Filter lines  /fi ERROR !debug", has_arg: true },
    SlashCommand { name: "time", description: "Jump to time  /t 14:30  /t -5m  /t +1h", has_arg: true },
    SlashCommand { name: "go", description: "Go to line or bookmark  /g 42  /g mymark", has_arg: true },
    SlashCommand { name: "bookmark", description: "Toggle bookmark  /b  or  /b name", has_arg: true },
    SlashCommand { name: "bookmarks", description: "Open bookmark list  /bs", has_arg: false },
    SlashCommand { name: "notify", description: "Watch for pattern  /n ERROR  (stacks)", has_arg: true },
    SlashCommand { name: "notifications", description: "List active notifications  /ns  d to remove", has_arg: false },
    SlashCommand { name: "delta", description: "Toggle time delta column  /d", has_arg: false },
    SlashCommand { name: "top", description: "Go to first line", has_arg: false },
    SlashCommand { name: "bottom", description: "Go to last line", has_arg: false },
    SlashCommand { name: "config", description: "Open settings  ←→ change  Esc save", has_arg: false },
    SlashCommand { name: "color", description: "Toggle semantic coloring  /c", has_arg: false },
    SlashCommand { name: "wrap", description: "Toggle line wrapping  /w", has_arg: false },
    SlashCommand { name: "help", description: "Show keyboard shortcuts & commands", has_arg: false },
    SlashCommand { name: "quit", description: "Quit  /q", has_arg: false },
];

pub struct NotifyEntry {
    pub pattern: String,
    pub regex: Regex,
}

pub struct FilterCondition {
    pub regex: Regex,
    pub negated: bool,
}

pub struct TailView {
    pub lines: Vec<String>,
    pub levels: Vec<LogLevel>,
}

#[derive(Debug, Clone, Copy)]
pub struct TextPos {
    pub line: usize,
    pub col: usize,
}

pub struct App {
    pub source: LogSource,
    pub filename: String,
    pub scroll_offset: usize,
    pub selected_lines: BTreeSet<usize>,
    pub selection_anchor: Option<usize>,
    pub drag_origin: Option<(u16, u16)>,
    pub text_selection: Option<(TextPos, TextPos)>,
    pub input: String,
    pub input_cursor: usize,
    pub input_mode: InputMode,
    pub search: SearchState,
    pub live_regex: Option<Regex>,
    pub should_quit: bool,
    pub viewport_height: usize,
    pub command_suggestions: Vec<usize>,
    pub suggestion_index: Option<usize>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub bookmarks: Vec<(usize, String)>,
    pub filter_conditions: Vec<FilterCondition>,
    pub filter_highlight: Option<Regex>,
    pub filtered_lines: Vec<usize>,
    pub filter_cursor: usize,
    pub filter_total: usize,
    pub filtering: bool,
    pub follow_mode: bool,
    pub show_delta: bool,
    pub show_help: bool,
    pub show_bookmarks: bool,
    pub bookmark_cursor: usize,
    pub status_message: Option<(String, bool)>,
    pub file_path: Option<PathBuf>,
    pub notify_entries: Vec<NotifyEntry>,
    pub show_notifications: bool,
    pub notification_cursor: usize,
    pub show_config: bool,
    pub config_cursor: usize,
    pub config: Config,
    pub wrap_mode: bool,
    pub wrap_row_map: Vec<usize>,
    pub wrap_char_offsets: Vec<usize>,
    pub last_click: Option<(Instant, u16)>,
    pub semantic_color: bool,
    pub tail_view: Option<TailView>,

    worker_tx: Option<Sender<WorkRequest>>,
    worker_rx: Option<Receiver<WorkResult>>,
    worker_handle: Option<JoinHandle<()>>,
    pub worker_busy: bool,

    search_generation: Generation,
    pub search_cancel: Option<Arc<AtomicBool>>,
    filter_generation: Generation,
    pub filter_cancel: Option<Arc<AtomicBool>>,

    shared_offsets: Option<Arc<Vec<u64>>>,
}

impl App {
    pub fn new(source: LogSource, filename: String, file_path: Option<PathBuf>, config: Config) -> Self {
        let (worker_tx, worker_rx, worker_handle) = if source.is_mmap() {
            let (req_tx, req_rx) = mpsc::channel();
            let (res_tx, res_rx) = mpsc::channel();
            let handle = std::thread::spawn(move || worker::worker_loop(req_rx, res_tx));
            (Some(req_tx), Some(res_rx), Some(handle))
        } else {
            (None, None, None)
        };

        Self {
            source,
            filename,
            scroll_offset: 0,
            selected_lines: BTreeSet::new(),
            selection_anchor: None,
            drag_origin: None,
            text_selection: None,
            input: String::new(),
            input_cursor: 0,
            input_mode: InputMode::Idle,
            search: SearchState::new(),
            live_regex: None,
            should_quit: false,
            viewport_height: 20,
            command_suggestions: Vec::new(),
            suggestion_index: None,
            input_history: Vec::new(),
            history_index: None,
            bookmarks: Vec::new(),
            filter_conditions: Vec::new(),
            filter_highlight: None,
            filtered_lines: Vec::new(),
            filter_cursor: 0,
            filter_total: 0,
            filtering: false,
            follow_mode: false,
            show_delta: false,
            show_help: false,
            show_bookmarks: false,
            bookmark_cursor: 0,
            status_message: None,
            file_path,
            notify_entries: Vec::new(),
            show_notifications: false,
            notification_cursor: 0,
            show_config: false,
            config_cursor: 0,
            wrap_mode: config.wrap,
            semantic_color: config.semantic_color,
            config,
            wrap_row_map: Vec::new(),
            wrap_char_offsets: Vec::new(),
            last_click: None,
            tail_view: None,
            worker_tx,
            worker_rx,
            worker_handle,
            worker_busy: false,
            search_generation: 0,
            search_cancel: None,
            filter_generation: 0,
            filter_cancel: None,
            shared_offsets: None,
        }
    }

    pub fn total_lines(&self) -> usize {
        self.source.index().total_lines
    }

    pub fn visible_count(&self) -> usize {
        if !self.filter_conditions.is_empty() {
            self.filtered_lines.len()
        } else {
            self.total_lines()
        }
    }

    pub fn actual_line(&self, visible_idx: usize) -> usize {
        if !self.filter_conditions.is_empty() {
            self.filtered_lines.get(visible_idx).copied().unwrap_or(0)
        } else {
            visible_idx
        }
    }

    pub fn is_bookmarked(&self, line: usize) -> bool {
        self.bookmarks.binary_search_by_key(&line, |(l, _)| *l).is_ok()
    }

    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(ref path) = self.file_path {
            let old_len = self.source.data().len();
            if let Ok(true) = self.source.reload(path) {
                changed = true;
                if !self.notify_entries.is_empty() {
                    let new_data = &self.source.data()[old_len..];
                    let text = String::from_utf8_lossy(new_data);
                    for entry in &self.notify_entries {
                        let matches: Vec<&str> = text.lines()
                            .filter(|l| entry.regex.is_match(l))
                            .collect();
                        if !matches.is_empty() {
                            let body = if matches.len() == 1 {
                                matches[0].to_string()
                            } else {
                                format!("{} matches found", matches.len())
                            };
                            send_notification(
                                &format!("loghew: \"{}\"", entry.pattern),
                                &body,
                            );
                        }
                    }
                }
            }
        }
        if self.follow_mode {
            self.scroll_to_bottom();
            changed = true;
        }
        changed
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let max = self.visible_count().saturating_sub(self.viewport_height);
        self.scroll_offset = (self.scroll_offset + lines).min(max);
    }

    pub fn scroll_up(&mut self, lines: usize) {
        if self.tail_view.is_some() {
            self.exit_tail_mode();
            return;
        }
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        if self.follow_mode && lines > 0 {
            self.follow_mode = false;
        }
    }

    pub fn scroll_to(&mut self, line: usize) {
        let max = self.visible_count().saturating_sub(self.viewport_height);
        self.scroll_offset = line.min(max);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        let max = self.visible_count().saturating_sub(self.viewport_height);
        self.scroll_offset = max;
    }

    pub fn jump_to_bottom(&mut self) {
        if self.source.scanning() {
            self.refresh_tail_view();
        } else {
            self.tail_view = None;
            self.scroll_to_bottom();
        }
    }

    fn refresh_tail_view(&mut self) {
        let count = self.viewport_height + 20;
        let pairs = if let Some(ref path) = self.file_path {
            crate::log::read_file_tail(path, count)
        } else {
            self.source.scan_tail(count)
        };
        let (lines, levels): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        self.tail_view = Some(TailView { lines, levels });
    }

    pub fn in_tail_mode(&self) -> bool {
        self.tail_view.is_some()
    }

    pub fn exit_tail_mode(&mut self) {
        self.tail_view = None;
        self.scroll_to_bottom();
    }

    fn visible_idx_for_row(&self, row: u16) -> usize {
        if self.wrap_mode && !self.wrap_row_map.is_empty() {
            self.wrap_row_map.get(row as usize).copied().unwrap_or(self.scroll_offset)
        } else {
            self.scroll_offset + row as usize
        }
    }

    pub fn click_line(&mut self, row: u16) {
        let visible_idx = self.visible_idx_for_row(row);
        let line_num = self.actual_line(visible_idx);
        if line_num >= self.total_lines() {
            return;
        }
        if self.selected_lines.len() == 1 && self.selected_lines.contains(&line_num) {
            self.selected_lines.clear();
            self.selection_anchor = None;
        } else {
            self.selected_lines.clear();
            self.selected_lines.insert(line_num);
            self.selection_anchor = Some(line_num);
        }
    }

    pub fn double_click_line(&mut self, row: u16) {
        let visible_idx = self.visible_idx_for_row(row);
        let line_num = self.actual_line(visible_idx);
        if line_num >= self.total_lines() {
            return;
        }
        let (start, end) = self.log_entry_range(line_num);
        self.selected_lines.clear();
        for l in start..=end {
            self.selected_lines.insert(l);
        }
        self.selection_anchor = Some(start);
    }

    fn log_entry_range(&self, line_num: usize) -> (usize, usize) {
        let index = self.source.index();
        if index.timestamp_format.is_none() || index.is_entry_start.is_empty() {
            return (line_num, line_num);
        }
        // Walk backwards to the entry start
        let mut start = line_num;
        while start > 0 && !index.is_entry_start.get(start).copied().unwrap_or(true) {
            start -= 1;
        }
        // Walk forward to find the next entry start
        let total = self.total_lines();
        let mut end = line_num + 1;
        while end < total && !index.is_entry_start.get(end).copied().unwrap_or(true) {
            end += 1;
        }
        (start, end - 1)
    }

    pub fn ctrl_click_line(&mut self, row: u16) {
        let visible_idx = self.visible_idx_for_row(row);
        let line_num = self.actual_line(visible_idx);
        if line_num >= self.total_lines() {
            return;
        }
        if self.selected_lines.contains(&line_num) {
            self.selected_lines.remove(&line_num);
        } else {
            self.selected_lines.insert(line_num);
        }
        self.selection_anchor = Some(line_num);
    }

    pub fn shift_click_line(&mut self, row: u16) {
        let visible_idx = self.visible_idx_for_row(row);
        let line_num = self.actual_line(visible_idx);
        if line_num >= self.total_lines() {
            return;
        }
        let anchor = self.selection_anchor.unwrap_or(line_num);
        let (start, end) = if anchor <= line_num {
            (anchor, line_num)
        } else {
            (line_num, anchor)
        };
        self.selected_lines.clear();
        for l in start..=end {
            if l < self.total_lines() {
                self.selected_lines.insert(l);
            }
        }
    }

    pub fn copy_selection(&self) -> Option<String> {
        if self.selected_lines.is_empty() {
            return None;
        }
        let lines: Vec<&str> = self
            .selected_lines
            .iter()
            .filter_map(|&i| self.source.get_line(i))
            .collect();
        Some(lines.join("\n"))
    }

    pub fn content_col_offset(&self) -> u16 {
        let gutter_width = if self.config.line_numbers {
            if self.total_lines() == 0 {
                1
            } else {
                (self.total_lines() as f64).log10().floor() as usize + 1
            }
            .max(1)
        } else {
            0
        };
        // bookmark(1) + gutter + separator(3) + delta(9 or 0)
        (1 + gutter_width + 3 + if self.show_delta { 9 } else { 0 }) as u16
    }

    fn terminal_to_text_pos(&self, row: u16, col: u16) -> Option<TextPos> {
        let visible_idx = self.visible_idx_for_row(row);
        if visible_idx >= self.visible_count() {
            return None;
        }
        let line = self.actual_line(visible_idx);
        let offset = self.content_col_offset();
        let text_col = if col >= offset {
            (col - offset) as usize
        } else {
            0
        };
        let char_offset = if self.wrap_mode {
            self.wrap_char_offsets.get(row as usize).copied().unwrap_or(0)
        } else {
            0
        };
        let line_len = self.source.get_line(line).map(|l| l.chars().count()).unwrap_or(0);
        Some(TextPos {
            line,
            col: (text_col + char_offset).min(line_len),
        })
    }

    pub fn start_drag(&mut self, row: u16, col: u16) {
        self.drag_origin = Some((row, col));
        self.text_selection = None;
    }

    pub fn update_drag(&mut self, row: u16, col: u16) {
        if let Some((start_row, start_col)) = self.drag_origin {
            if row == start_row && col == start_col {
                return;
            }
            if let (Some(start), Some(end)) = (
                self.terminal_to_text_pos(start_row, start_col),
                self.terminal_to_text_pos(row, col),
            ) {
                let (s, e) = if start.line < end.line
                    || (start.line == end.line && start.col <= end.col)
                {
                    (start, end)
                } else {
                    (end, start)
                };
                self.text_selection = Some((s, e));
                self.selected_lines.clear();
            }
        }
    }

    pub fn end_drag(&mut self) {
        self.drag_origin = None;
    }

    pub fn selected_text(&self) -> Option<String> {
        if let Some((start, end)) = self.text_selection {
            return self.extract_text_range(start, end);
        }
        self.copy_selection()
    }

    fn extract_text_range(&self, start: TextPos, end: TextPos) -> Option<String> {
        if start.line == end.line {
            let line = self.source.get_line(start.line)?;
            let sc = char_to_byte(line, start.col);
            let ec = char_to_byte(line, end.col);
            if sc < ec {
                return Some(line[sc..ec].to_string());
            }
            return None;
        }
        let mut result = String::new();
        for l in start.line..=end.line {
            let line = self.source.get_line(l).unwrap_or("");
            if l == start.line {
                let sc = char_to_byte(line, start.col);
                result.push_str(&line[sc..]);
            } else if l == end.line {
                let ec = char_to_byte(line, end.col);
                result.push_str(&line[..ec]);
            } else {
                result.push_str(line);
            }
            if l < end.line {
                result.push('\n');
            }
        }
        Some(result)
    }

    pub fn clear_selection(&mut self) {
        self.selected_lines.clear();
        self.selection_anchor = None;
        self.text_selection = None;
        self.drag_origin = None;
    }

    pub fn line_text_selection(&self, line_num: usize) -> Option<(usize, usize)> {
        let (start, end) = self.text_selection?;
        if line_num < start.line || line_num > end.line {
            return None;
        }
        let line_len = self.source.get_line(line_num).map(|l| l.chars().count()).unwrap_or(0);
        let sc = if line_num == start.line { start.col } else { 0 };
        let ec = if line_num == end.line { end.col } else { line_len };
        Some((sc.min(line_len), ec.min(line_len)))
    }

    pub fn type_char(&mut self, ch: char) {
        if self.input_mode == InputMode::Idle {
            self.input_mode = InputMode::Typing;
            self.status_message = None;
        }
        self.input.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();

        if self.input.starts_with('/') {
            self.update_suggestions();
            self.live_regex = None;
        } else {
            self.command_suggestions.clear();
            self.suggestion_index = None;
            self.update_live_highlight();
        }
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let prev = self.input[..self.input_cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input.remove(prev);
        self.input_cursor = prev;

        if self.input.is_empty() {
            self.input_mode = InputMode::Idle;
            self.command_suggestions.clear();
            self.suggestion_index = None;
            self.live_regex = None;
        } else if self.input.starts_with('/') {
            self.update_suggestions();
            self.live_regex = None;
        } else {
            self.command_suggestions.clear();
            self.suggestion_index = None;
            self.update_live_highlight();
        }
    }

    pub fn input_delete(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input.remove(self.input_cursor);
            if self.input.is_empty() {
                self.input_mode = InputMode::Idle;
                self.command_suggestions.clear();
                self.suggestion_index = None;
                self.live_regex = None;
            } else if self.input.starts_with('/') {
                self.update_suggestions();
            } else {
                self.update_live_highlight();
            }
        }
    }

    pub fn input_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self.input[..self.input_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn input_right(&mut self) {
        if self.input_cursor < self.input.len() {
            self.input_cursor = self.input[self.input_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.input_cursor + i)
                .unwrap_or(self.input.len());
        }
    }

    pub fn input_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn input_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    pub fn cancel_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.input_mode = InputMode::Idle;
        self.command_suggestions.clear();
        self.suggestion_index = None;
        self.history_index = None;
        self.live_regex = None;
        if let Some(cancel) = &self.search_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.search = crate::search::SearchState::new();
    }

    pub fn submit_input(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }

        self.push_history(text.clone());

        if text.starts_with('/') {
            self.execute_command(&text);
        } else {
            self.execute_search(&text);
        }

        self.input.clear();
        self.input_cursor = 0;
        self.input_mode = InputMode::Idle;
        self.command_suggestions.clear();
        self.suggestion_index = None;
        self.history_index = None;
        self.live_regex = None;
    }

    fn execute_search(&mut self, pattern: &str) {
        self.search.set_literal(pattern);
        if self.search.error.is_some() {
            return;
        }
        if let Some(cancel) = &self.search_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.search_generation += 1;
        let total = self.source.index().total_lines;
        self.search.start_search(total);
        if self.worker_tx.is_some() {
            self.snapshot_offsets();
            self.search_cancel = Some(Arc::new(AtomicBool::new(false)));
        }
        self.set_status("Searching...", false);
    }

    fn execute_regex_search(&mut self, pattern: &str) {
        self.search.set_regex(pattern);
        if self.search.error.is_some() {
            return;
        }
        if let Some(cancel) = &self.search_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.search_generation += 1;
        let total = self.source.index().total_lines;
        self.search.start_search(total);
        if self.worker_tx.is_some() {
            self.snapshot_offsets();
            self.search_cancel = Some(Arc::new(AtomicBool::new(false)));
        }
        self.set_status("Searching...", false);
    }

    pub fn searching(&self) -> bool {
        self.search.searching
    }

    pub fn search_tick(&mut self) -> bool {
        let source = &self.source;
        let still_going = self.search.search_batch(10_000, |i| source.get_line(i));
        if still_going {
            let pct = if self.search.search_total > 0 {
                self.search.search_cursor * 100 / self.search.search_total
            } else {
                100
            };
            self.status_message = Some((
                format!("Searching... {}%  ({} matches)", pct, self.search.matches.len()),
                false,
            ));
        } else {
            let count = self.search.match_count();
            self.status_message = Some((
                format!("{} matches for \"{}\"", count, self.search.pattern),
                false,
            ));
            if let Some(line) = self.search.jump_to_nearest(self.scroll_offset) {
                self.scroll_to(line);
            }
        }
        still_going
    }

    fn execute_command(&mut self, input: &str) {
        let input = input.strip_prefix('/').unwrap_or(input);
        let mut parts = input.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match cmd {
            "regex" | "r" => {
                if !arg.is_empty() {
                    self.execute_regex_search(arg);
                }
            }
            "quit" | "q" | "exit" | "x" => self.should_quit = true,
            "top" => self.scroll_to_top(),
            "bottom" | "end" => self.jump_to_bottom(),
            "help" => self.show_help = !self.show_help,
            "follow" | "f" => {
                self.follow_mode = !self.follow_mode;
                if self.follow_mode {
                    self.jump_to_bottom();
                    // Don't set status_message — the status bar shows FOLLOWING + SCANNING%
                    self.status_message = None;
                } else {
                    self.set_status("Follow mode OFF", false);
                }
            }
            "color" | "c" => {
                self.semantic_color = !self.semantic_color;
                self.set_status(if self.semantic_color { "Semantic coloring ON" } else { "Semantic coloring OFF" }, false);
            }
            "wrap" | "w" => {
                self.wrap_mode = !self.wrap_mode;
                self.set_status(if self.wrap_mode { "Line wrap ON" } else { "Line wrap OFF" }, false);
            }
            "delta" | "d" => {
                self.show_delta = !self.show_delta;
                if self.show_delta {
                    self.set_status("Time deltas ON", false);
                } else {
                    self.set_status("Time deltas OFF", false);
                }
            }
            "filter" | "fi" | "only-show" => {
                if arg.is_empty() {
                    self.clear_filter();
                } else {
                    self.apply_filter(arg);
                }
            }
            "time" | "t" => {
                if !arg.is_empty() {
                    self.jump_to_time(arg);
                } else {
                    self.set_status("Usage: /time HH:MM or /time -5m", true);
                }
            }
            "go" | "g" => {
                if arg.is_empty() {
                    self.set_status("Usage: /go <line> or /go <bookmark>", true);
                } else if let Ok(n) = arg.parse::<usize>() {
                    let line = n.saturating_sub(1).min(self.total_lines().saturating_sub(1));
                    self.scroll_to(line);
                    self.set_status(format!("Jumped to line {}", line + 1), false);
                } else if let Some((line, label)) = self.bookmarks.iter().find(|(_, l)| l == arg).cloned() {
                    self.scroll_to(line);
                    self.set_status(format!("→ {}", label), false);
                } else {
                    self.set_status(format!("No bookmark named \"{}\"", arg), true);
                }
            }
            "bookmark" | "bm" | "b" => {
                self.add_bookmark(arg);
            }
            "bookmarks" | "bs" => {
                self.open_bookmarks();
            }
            "notify" | "n" => {
                if arg.is_empty() {
                    self.open_notifications();
                } else {
                    let pat = format!("(?i){}", regex::escape(arg));
                    match Regex::new(&pat) {
                        Ok(re) => {
                            self.notify_entries.push(NotifyEntry {
                                pattern: arg.to_string(),
                                regex: re,
                            });
                            self.set_status(
                                format!("Notify added for \"{}\"", arg),
                                false,
                            );
                        }
                        Err(e) => {
                            self.set_status(format!("Invalid pattern: {}", e), true);
                        }
                    }
                }
            }
            "notifications" | "ns" => {
                self.open_notifications();
            }
            "config" | "settings" => {
                self.show_config = true;
                self.config_cursor = 0;
            }
            "theme" => {
                self.set_status("Theme: default (more themes coming soon)", false);
            }
            _ => {
                self.set_status(format!("Unknown command: /{}", cmd), true);
            }
        }
    }

    fn apply_filter(&mut self, pattern: &str) {
        let words: Vec<&str> = pattern.split_whitespace().collect();
        let mut conditions = Vec::new();
        let mut positive_terms = Vec::new();

        for word in &words {
            let (negated, term) = if let Some(rest) = word.strip_prefix('!') {
                (true, rest)
            } else {
                (false, *word)
            };
            if term.is_empty() {
                continue;
            }
            let pat = format!("(?i){}", regex::escape(term));
            match Regex::new(&pat) {
                Ok(re) => {
                    if !negated {
                        positive_terms.push(regex::escape(term));
                    }
                    conditions.push(FilterCondition { regex: re, negated });
                }
                Err(e) => {
                    self.set_status(format!("Invalid pattern: {}", e), true);
                    return;
                }
            }
        }

        if conditions.is_empty() {
            self.clear_filter();
            return;
        }

        let highlight = if positive_terms.is_empty() {
            None
        } else {
            Regex::new(&format!("(?i){}", positive_terms.join("|"))).ok()
        };

        if let Some(cancel) = &self.filter_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.filter_generation += 1;
        self.filtered_lines.clear();
        self.filter_conditions = conditions;
        self.filter_highlight = highlight;
        self.filter_cursor = 0;
        self.filter_total = self.total_lines();
        self.filtering = true;
        self.scroll_offset = 0;
        if self.worker_tx.is_some() {
            self.snapshot_offsets();
            self.filter_cancel = Some(Arc::new(AtomicBool::new(false)));
        }
        self.set_status("Filtering...", false);
    }

    pub fn filter_tick(&mut self) -> bool {
        if !self.filtering {
            return false;
        }
        let end = (self.filter_cursor + 10_000).min(self.filter_total);
        for i in self.filter_cursor..end {
            if let Some(line) = self.source.get_line(i) {
                let matches = self.filter_conditions.iter().all(|c| {
                    let found = c.regex.is_match(line);
                    if c.negated { !found } else { found }
                });
                if matches {
                    self.filtered_lines.push(i);
                }
            }
        }
        self.filter_cursor = end;
        if end >= self.filter_total {
            self.filtering = false;
            let count = self.filtered_lines.len();
            self.set_status(
                format!("Showing {} filtered lines", count),
                false,
            );
        } else {
            let pct = self.filter_cursor * 100 / self.filter_total;
            self.status_message = Some((
                format!("Filtering... {}%  ({} matches)", pct, self.filtered_lines.len()),
                false,
            ));
        }
        self.filtering
    }

    fn clear_filter(&mut self) {
        if !self.filter_conditions.is_empty() || self.filtering {
            if let Some(cancel) = &self.filter_cancel {
                cancel.store(true, Ordering::Relaxed);
            }
            self.filter_conditions.clear();
            self.filter_highlight = None;
            self.filtered_lines.clear();
            self.filtering = false;
            self.scroll_offset = 0;
            self.set_status("Filter cleared", false);
        } else {
            self.set_status("No active filter", false);
        }
    }

    fn jump_to_time(&mut self, input: &str) {
        let index = self.source.index();
        if index.timestamp_format.is_none() {
            self.set_status("No timestamps detected in file", true);
            return;
        }

        if input.starts_with('+') || input.starts_with('-') {
            self.jump_relative_time(input);
            return;
        }

        let base_date = index.timestamps.iter().find_map(|t| *t).unwrap_or(0);

        let target_ms = if let Ok(t) = chrono::NaiveTime::parse_from_str(input, "%H:%M:%S") {
            let base_dt = chrono::DateTime::from_timestamp_millis(base_date).unwrap_or_default();
            base_dt.date_naive().and_time(t).and_utc().timestamp_millis()
        } else if let Ok(t) = chrono::NaiveTime::parse_from_str(input, "%H:%M") {
            let base_dt = chrono::DateTime::from_timestamp_millis(base_date).unwrap_or_default();
            base_dt.date_naive().and_time(t).and_utc().timestamp_millis()
        } else if let Ok(dt) =
            chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S")
        {
            dt.and_utc().timestamp_millis()
        } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M") {
            dt.and_utc().timestamp_millis()
        } else {
            self.set_status(format!("Cannot parse time: {}", input), true);
            return;
        };

        let line = self.binary_search_timestamp(target_ms);
        self.scroll_to(line);
        self.set_status(format!("Jumped to {}", input), false);
    }

    fn jump_relative_time(&mut self, input: &str) {
        let index = self.source.index();
        let current_line = self.actual_line(self.scroll_offset);
        let current_ts = index
            .timestamps
            .get(current_line)
            .and_then(|t| *t)
            .unwrap_or(0);

        let (sign, rest) = if input.starts_with('-') {
            (-1i64, &input[1..])
        } else {
            (1i64, &input[1..])
        };

        let (num_str, unit) = if rest.ends_with('s') || rest.ends_with('m') || rest.ends_with('h') {
            rest.split_at(rest.len() - 1)
        } else {
            self.set_status("Use s/m/h suffix (e.g., -5m, +30s, -1h)", true);
            return;
        };

        let num: i64 = match num_str.parse() {
            Ok(n) => n,
            Err(_) => {
                self.set_status(format!("Invalid time offset: {}", input), true);
                return;
            }
        };

        let ms_offset = match unit {
            "s" => num * 1000,
            "m" => num * 60_000,
            "h" => num * 3_600_000,
            _ => unreachable!(),
        };

        let target_ms = current_ts + sign * ms_offset;
        let line = self.binary_search_timestamp(target_ms);
        self.scroll_to(line);
        self.set_status(format!("Jumped {}", input), false);
    }

    fn binary_search_timestamp(&self, target_ms: i64) -> usize {
        let timestamps = &self.source.index().timestamps;
        let mut lo = 0usize;
        let mut hi = timestamps.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let ts = timestamps[mid].unwrap_or(i64::MIN);
            if ts < target_ms {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.min(self.total_lines().saturating_sub(1))
    }

    fn add_bookmark(&mut self, name: &str) {
        if !name.is_empty() && name.parse::<usize>().is_ok() {
            self.set_status("Bookmark name can't be a number", true);
            return;
        }
        let line = self.selected_lines.iter().next().copied()
            .unwrap_or(self.actual_line(self.scroll_offset));
        if self.bookmarks.iter().any(|(l, _)| *l == line) {
            self.bookmarks.retain(|(l, _)| *l != line);
            self.set_status(format!("Bookmark removed from line {}", line + 1), false);
            return;
        }
        let label = if name.is_empty() {
            let text = self.source.get_line(line).unwrap_or("");
            let preview: String = text.chars().take(30).collect();
            format!("L{}: {}", line + 1, preview)
        } else {
            name.to_string()
        };
        self.bookmarks.push((line, label.clone()));
        self.bookmarks.sort_by_key(|(l, _)| *l);
        self.set_status(format!("Bookmarked: {}", label), false);
    }

    pub fn open_bookmarks(&mut self) {
        if self.bookmarks.is_empty() {
            self.set_status("No bookmarks set", true);
            return;
        }
        self.show_bookmarks = true;
        self.bookmark_cursor = 0;
    }

    pub fn bookmark_up(&mut self) {
        if self.bookmark_cursor > 0 {
            self.bookmark_cursor -= 1;
        }
    }

    pub fn bookmark_down(&mut self) {
        if self.bookmark_cursor + 1 < self.bookmarks.len() {
            self.bookmark_cursor += 1;
        }
    }

    pub fn bookmark_select(&mut self) {
        if let Some((line, label)) = self.bookmarks.get(self.bookmark_cursor).cloned() {
            self.show_bookmarks = false;
            self.scroll_to(line);
            self.set_status(format!("→ {}", label), false);
        }
    }

    pub fn bookmark_delete_selected(&mut self) {
        if self.bookmark_cursor < self.bookmarks.len() {
            self.bookmarks.remove(self.bookmark_cursor);
            if self.bookmarks.is_empty() {
                self.show_bookmarks = false;
            } else if self.bookmark_cursor >= self.bookmarks.len() {
                self.bookmark_cursor = self.bookmarks.len() - 1;
            }
        }
    }

    pub fn open_notifications(&mut self) {
        if self.notify_entries.is_empty() {
            self.set_status("No active notifications", true);
            return;
        }
        self.show_notifications = true;
        self.notification_cursor = 0;
    }

    pub fn notification_up(&mut self) {
        if self.notification_cursor > 0 {
            self.notification_cursor -= 1;
        }
    }

    pub fn notification_down(&mut self) {
        if self.notification_cursor + 1 < self.notify_entries.len() {
            self.notification_cursor += 1;
        }
    }

    pub fn notification_delete_selected(&mut self) {
        if self.notification_cursor < self.notify_entries.len() {
            let removed = self.notify_entries.remove(self.notification_cursor);
            if self.notify_entries.is_empty() {
                self.show_notifications = false;
                self.set_status(format!("Removed \"{}\", no notifications left", removed.pattern), false);
            } else {
                if self.notification_cursor >= self.notify_entries.len() {
                    self.notification_cursor = self.notify_entries.len() - 1;
                }
            }
        }
    }

    pub const CONFIG_ITEMS: [&'static str; 6] = ["Theme", "Line numbers", "Mouse", "Scroll speed", "Wrap", "Semantic color"];

    pub fn config_up(&mut self) {
        if self.config_cursor > 0 {
            self.config_cursor -= 1;
        }
    }

    pub fn config_down(&mut self) {
        if self.config_cursor + 1 < Self::CONFIG_ITEMS.len() {
            self.config_cursor += 1;
        }
    }

    pub fn config_toggle(&mut self) {
        match self.config_cursor {
            0 => {
                let next = (self.config.theme_index + 1) % config::PRESETS.len();
                self.config.apply_preset(next);
            }
            1 => self.config.line_numbers = !self.config.line_numbers,
            2 => self.config.mouse = !self.config.mouse,
            3 => self.config.scroll_speed = (self.config.scroll_speed + 1).min(10),
            4 => {
                self.config.wrap = !self.config.wrap;
                self.wrap_mode = self.config.wrap;
            }
            5 => {
                self.config.semantic_color = !self.config.semantic_color;
                self.semantic_color = self.config.semantic_color;
            }
            _ => {}
        }
    }

    pub fn config_decrease(&mut self) {
        match self.config_cursor {
            0 => {
                let prev = if self.config.theme_index == 0 {
                    config::PRESETS.len() - 1
                } else {
                    self.config.theme_index - 1
                };
                self.config.apply_preset(prev);
            }
            3 => self.config.scroll_speed = self.config.scroll_speed.saturating_sub(1).max(1),
            _ => {}
        }
    }

    pub fn config_value(&self, idx: usize) -> String {
        match idx {
            0 => self.config.theme_name().to_string(),
            1 => if self.config.line_numbers { "ON" } else { "OFF" }.to_string(),
            2 => if self.config.mouse { "ON" } else { "OFF" }.to_string(),
            3 => self.config.scroll_speed.to_string(),
            4 => if self.config.wrap { "ON" } else { "OFF" }.to_string(),
            5 => if self.config.semantic_color { "ON" } else { "OFF" }.to_string(),
            _ => String::new(),
        }
    }

    pub fn save_config(&self) {
        let content = format!(
            "[general]\ntheme = \"{}\"\nscroll_speed = {}\nline_numbers = {}\nmouse = {}\nwrap = {}\nsemantic_color = {}\n",
            self.config.theme_name(), self.config.scroll_speed, self.config.line_numbers, self.config.mouse, self.config.wrap, self.config.semantic_color
        );
        if let Some(dir) = dirs::config_dir() {
            let dir = dir.join("loghew");
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join("config.toml"), content);
        }
    }

    // --- Scanning & deferred indexing ---

    pub fn is_scanning(&self) -> bool {
        self.source.scanning()
    }

    pub fn scan_tick(&mut self) {
        self.source.scan_batch();
    }

    pub fn parse_deferred_batch(&mut self) -> bool {
        self.source.parse_deferred_batch(10_000)
    }

    pub fn indexing_ready(&self) -> bool {
        self.source.indexing_ready()
    }

    // --- Worker thread ---

    fn snapshot_offsets(&mut self) {
        let offsets = self.source.index().line_offsets.clone();
        self.shared_offsets = Some(Arc::new(offsets));
    }

    pub fn poll_worker(&mut self) -> bool {
        if self.worker_rx.is_none() {
            return false;
        }

        let mut results = Vec::new();
        if let Some(rx) = &self.worker_rx {
            while let Ok(result) = rx.try_recv() {
                results.push(result);
            }
        }

        if results.is_empty() {
            return false;
        }

        for result in results {
            self.worker_busy = false;
            match result {
                WorkResult::ScanChunk { chunk, next_offset } => {
                    let is_empty = chunk.line_offsets.is_empty();
                    match &mut self.source {
                        LogSource::Mmap { index, scan_offset, scan_limit, .. } => {
                            if is_empty {
                                *scan_offset = *scan_limit;
                            } else {
                                index.merge_chunk(chunk);
                                *scan_offset = next_offset;
                            }
                            let scanning_done = *scan_offset >= *scan_limit;
                            if scanning_done && self.tail_view.is_some() {
                                self.tail_view = None;
                                self.scroll_to_bottom();
                            } else if self.follow_mode && self.in_tail_mode() {
                                self.refresh_tail_view();
                            } else if self.follow_mode {
                                self.scroll_to_bottom();
                            }
                        }
                        _ => {}
                    }
                    if self.follow_mode && self.source.scanning() {
                        self.status_message = None;
                    }
                }
                WorkResult::DeferredParsed { start_index, timestamps, levels, is_entry_start, level_counts_delta, last_parsed_ts } => {
                    let batch_len = timestamps.len();
                    self.source.index_mut().apply_deferred_batch(
                        start_index, timestamps, levels, is_entry_start, level_counts_delta,
                    );
                    let new_cursor = start_index + batch_len;
                    self.source.index_mut().set_parse_cursor(new_cursor, last_parsed_ts);
                }
                WorkResult::SearchBatch { matches, cursor, done, generation } => {
                    if generation != self.search_generation {
                        continue;
                    }
                    self.search.matches.extend(matches);
                    self.search.search_cursor = cursor;
                    if done {
                        self.search.searching = false;
                        let count = self.search.match_count();
                        self.status_message = Some((
                            format!("{} matches for \"{}\"", count, self.search.pattern),
                            false,
                        ));
                        if let Some(line) = self.search.jump_to_nearest(self.scroll_offset) {
                            self.scroll_to(line);
                        }
                    } else {
                        let pct = if self.search.search_total > 0 {
                            self.search.search_cursor * 100 / self.search.search_total
                        } else {
                            100
                        };
                        self.status_message = Some((
                            format!("Searching... {}%  ({} matches)", pct, self.search.matches.len()),
                            false,
                        ));
                    }
                }
                WorkResult::FilterBatch { matches, cursor, done, generation } => {
                    if generation != self.filter_generation {
                        continue;
                    }
                    self.filtered_lines.extend(matches);
                    self.filter_cursor = cursor;
                    if done {
                        self.filtering = false;
                        let count = self.filtered_lines.len();
                        self.status_message = Some((
                            format!("Showing {} filtered lines", count),
                            false,
                        ));
                    } else {
                        let pct = self.filter_cursor * 100 / self.filter_total;
                        self.status_message = Some((
                            format!("Filtering... {}%  ({} matches)", pct, self.filtered_lines.len()),
                            false,
                        ));
                    }
                }
            }
        }
        true
    }

    pub fn submit_next_work(&mut self) {
        let tx = match &self.worker_tx {
            Some(tx) => tx,
            None => {
                self.submit_sync_work();
                return;
            }
        };

        let mmap = match self.source.mmap_arc() {
            Some(m) => m,
            None => return,
        };

        // Priority: searching > filtering > scanning > deferred_parse
        if self.searching() {
            let re = match &self.search.regex {
                Some(r) => r.clone(),
                None => return,
            };
            let offsets = match &self.shared_offsets {
                Some(o) => Arc::clone(o),
                None => return,
            };
            let cancel = self.search_cancel.clone().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            let data_len = mmap.len();
            let _ = tx.send(WorkRequest::SearchBatch {
                mmap,
                regex: re,
                line_offsets: offsets,
                data_len,
                start_line: self.search.search_cursor,
                batch_size: 50_000,
                generation: self.search_generation,
                cancel,
            });
            self.worker_busy = true;
        } else if self.filtering {
            let conditions: Vec<(Regex, bool)> = self.filter_conditions.iter()
                .map(|c| (c.regex.clone(), c.negated))
                .collect();
            let offsets = match &self.shared_offsets {
                Some(o) => Arc::clone(o),
                None => return,
            };
            let cancel = self.filter_cancel.clone().unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
            let data_len = mmap.len();
            let _ = tx.send(WorkRequest::FilterBatch {
                mmap,
                conditions,
                line_offsets: offsets,
                data_len,
                start_line: self.filter_cursor,
                batch_size: 50_000,
                generation: self.filter_generation,
                cancel,
            });
            self.worker_busy = true;
        } else if self.is_scanning() {
            if let LogSource::Mmap { scan_offset, scan_limit, index, .. } = &self.source {
                let ts_format = index.timestamp_format.clone();
                let _ = tx.send(WorkRequest::ScanBatch {
                    mmap,
                    start_byte: *scan_offset,
                    scan_limit: *scan_limit,
                    ts_format,
                    max_lines: 50_000,
                });
                self.worker_busy = true;
            }
        } else if !self.indexing_ready() {
            let index = self.source.index();
            let cursor = index.parse_cursor();
            let total = index.total_lines;
            let batch_size = 10_000.min(total.saturating_sub(cursor));
            if batch_size == 0 {
                return;
            }
            let offsets: Vec<u64> = index.line_offsets[cursor..cursor + batch_size].to_vec();
            let ts_format = index.timestamp_format.clone();
            let last_ts = index.last_parsed_ts();
            let _ = tx.send(WorkRequest::DeferredParseBatch {
                mmap,
                offsets,
                start_index: cursor,
                ts_format,
                last_ts,
            });
            self.worker_busy = true;
        }
    }

    fn submit_sync_work(&mut self) {
        if self.searching() {
            self.search_tick();
        } else if self.filtering {
            self.filter_tick();
        } else if self.is_scanning() {
            self.scan_tick();
            if !self.is_scanning() && self.tail_view.is_some() {
                self.tail_view = None;
                self.scroll_to_bottom();
            } else if self.follow_mode && !self.in_tail_mode() {
                self.scroll_to_bottom();
            }
        } else if !self.indexing_ready() {
            self.parse_deferred_batch();
        }
    }

    pub fn shutdown_worker(&mut self) {
        if let Some(tx) = self.worker_tx.take() {
            let _ = tx.send(WorkRequest::Quit);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.worker_rx = None;
    }

    // --- Status ---

    fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_message = Some((msg.into(), is_error));
    }

    fn update_live_highlight(&mut self) {
        if self.input.is_empty() || self.input.starts_with('/') {
            self.live_regex = None;
            return;
        }
        let escaped = format!("(?i){}", regex::escape(&self.input));
        self.live_regex = Regex::new(&escaped).ok();
    }

    pub fn highlight_regex(&self) -> Option<&Regex> {
        if self.input_mode == InputMode::Typing {
            self.live_regex.as_ref()
        } else if self.search.regex.is_some() {
            self.search.regex.as_ref()
        } else {
            self.filter_highlight.as_ref()
        }
    }

    pub fn next_match(&mut self) {
        if let Some(line) = self.search.next_match() {
            self.scroll_to(line);
        }
    }

    pub fn prev_match(&mut self) {
        if let Some(line) = self.search.prev_match() {
            self.scroll_to(line);
        }
    }

    pub fn has_active_search(&self) -> bool {
        !self.search.pattern.is_empty()
    }

    fn update_suggestions(&mut self) {
        let query = self.input.strip_prefix('/').unwrap_or(&self.input);
        self.command_suggestions.clear();

        if query.is_empty() {
            self.command_suggestions = (0..SLASH_COMMANDS.len()).collect();
            self.suggestion_index = Some(0);
            return;
        }

        let query_lower = query.to_lowercase();
        if query_lower.contains(' ') {
            self.suggestion_index = None;
            return;
        }

        let mut prefix_matches = Vec::new();
        let mut fuzzy_matches = Vec::new();
        for (i, cmd) in SLASH_COMMANDS.iter().enumerate() {
            if cmd.name.starts_with(&query_lower) {
                prefix_matches.push(i);
            } else if fuzzy_match(cmd.name, &query_lower) {
                fuzzy_matches.push(i);
            }
        }
        self.command_suggestions.extend(prefix_matches);
        self.command_suggestions.extend(fuzzy_matches);

        self.suggestion_index = if self.command_suggestions.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn suggestion_next(&mut self) {
        let max = self.command_suggestions.len().min(10);
        if max == 0 {
            return;
        }
        let idx = match self.suggestion_index {
            Some(i) if i + 1 < max => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.suggestion_index = Some(idx);
    }

    pub fn suggestion_prev(&mut self) {
        let max = self.command_suggestions.len().min(10);
        if max == 0 {
            return;
        }
        let idx = match self.suggestion_index {
            Some(i) if i == 0 => max - 1,
            Some(i) => i - 1,
            None => max - 1,
        };
        self.suggestion_index = Some(idx);
    }

    pub fn accept_suggestion(&mut self) {
        let idx = match self.suggestion_index {
            Some(i) => i,
            None => return,
        };
        if idx >= self.command_suggestions.len() {
            return;
        }
        let cmd_idx = self.command_suggestions[idx];
        let cmd = &SLASH_COMMANDS[cmd_idx];
        if cmd.has_arg {
            self.input = format!("/{} ", cmd.name);
            self.input_cursor = self.input.len();
            self.command_suggestions.clear();
            self.suggestion_index = None;
        } else {
            self.input = format!("/{}", cmd.name);
            self.input_cursor = self.input.len();
            self.submit_input();
        }
    }

    fn push_history(&mut self, entry: String) {
        if self.input_history.last() != Some(&entry) {
            self.input_history.push(entry);
            if self.input_history.len() > 100 {
                self.input_history.remove(0);
            }
        }
        self.history_index = None;
    }
}

fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut chars = needle.chars().peekable();
    for h in haystack.chars() {
        if chars.peek() == Some(&h) {
            chars.next();
        }
    }
    chars.peek().is_none()
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn send_notification(title: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show();
}
