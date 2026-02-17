use regex::Regex;

#[derive(Debug)]
pub struct SearchState {
    pub pattern: String,
    pub regex: Option<Regex>,
    pub error: Option<String>,
    pub matches: Vec<usize>,
    pub current_match: Option<usize>,
    pub search_cursor: usize,
    pub search_total: usize,
    pub searching: bool,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            pattern: String::new(),
            regex: None,
            error: None,
            matches: Vec::new(),
            current_match: None,
            search_cursor: 0,
            search_total: 0,
            searching: false,
        }
    }

    pub fn set_literal(&mut self, pattern: &str) {
        self.pattern = pattern.to_string();
        self.error = None;
        self.matches.clear();
        self.current_match = None;

        if pattern.is_empty() {
            self.regex = None;
            return;
        }

        let escaped = format!("(?i){}", regex::escape(pattern));
        match Regex::new(&escaped) {
            Ok(re) => self.regex = Some(re),
            Err(e) => {
                self.regex = None;
                self.error = Some(format!("{e}"));
            }
        }
    }

    pub fn set_regex(&mut self, pattern: &str) {
        self.pattern = pattern.to_string();
        self.error = None;
        self.matches.clear();
        self.current_match = None;

        if pattern.is_empty() {
            self.regex = None;
            return;
        }

        match Regex::new(pattern) {
            Ok(re) => self.regex = Some(re),
            Err(e) => {
                self.regex = None;
                self.error = Some(format!("{e}"));
            }
        }
    }

    pub fn find_matches<'a, F>(&mut self, total_lines: usize, get_line: F)
    where
        F: Fn(usize) -> Option<&'a str>,
    {
        self.matches.clear();
        self.current_match = None;

        let re = match &self.regex {
            Some(r) => r,
            None => return,
        };

        for i in 0..total_lines {
            if let Some(line) = get_line(i) {
                if re.is_match(line) {
                    self.matches.push(i);
                }
            }
        }
    }

    pub fn start_search(&mut self, total_lines: usize) {
        self.matches.clear();
        self.current_match = None;
        self.search_cursor = 0;
        self.search_total = total_lines;
        self.searching = true;
    }

    pub fn search_batch<'a, F>(&mut self, batch_size: usize, get_line: F) -> bool
    where
        F: Fn(usize) -> Option<&'a str>,
    {
        let re = match &self.regex {
            Some(r) => r.clone(),
            None => {
                self.searching = false;
                return false;
            }
        };

        let end = (self.search_cursor + batch_size).min(self.search_total);
        for i in self.search_cursor..end {
            if let Some(line) = get_line(i) {
                if re.is_match(line) {
                    self.matches.push(i);
                }
            }
        }
        self.search_cursor = end;
        if end >= self.search_total {
            self.searching = false;
        }
        self.searching
    }

    /// Set current_match to the first match at or after `from_line`.
    pub fn jump_to_nearest(&mut self, from_line: usize) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let pos = self
            .matches
            .iter()
            .position(|&m| m >= from_line)
            .unwrap_or(0);
        self.current_match = Some(pos);
        Some(self.matches[pos])
    }

    /// Advance to next match (wraps around).
    pub fn next_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let pos = match self.current_match {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.current_match = Some(pos);
        Some(self.matches[pos])
    }

    /// Go to previous match (wraps around).
    pub fn prev_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let pos = match self.current_match {
            Some(i) => {
                if i == 0 {
                    self.matches.len() - 1
                } else {
                    i - 1
                }
            }
            None => self.matches.len() - 1,
        };
        self.current_match = Some(pos);
        Some(self.matches[pos])
    }

    pub fn current_match_line(&self) -> Option<usize> {
        self.current_match.map(|i| self.matches[i])
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn is_current_match_line(&self, line: usize) -> bool {
        self.current_match_line() == Some(line)
    }
}
