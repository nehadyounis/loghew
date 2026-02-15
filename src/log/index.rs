use super::parser::{detect_level, LogLevel, TimestampFormat};

#[derive(Debug)]
pub struct LogIndex {
    pub line_offsets: Vec<u64>,
    pub timestamps: Vec<Option<i64>>,
    pub levels: Vec<LogLevel>,
    pub is_entry_start: Vec<bool>,
    pub timestamp_format: Option<TimestampFormat>,
    pub total_lines: usize,
    pub level_counts: LevelCounts,
}

pub struct IndexChunk {
    pub line_offsets: Vec<u64>,
    pub timestamps: Vec<Option<i64>>,
    pub levels: Vec<LogLevel>,
    pub is_entry_start: Vec<bool>,
}

impl LogIndex {
    pub fn new() -> Self {
        Self {
            line_offsets: Vec::new(),
            timestamps: Vec::new(),
            levels: Vec::new(),
            is_entry_start: Vec::new(),
            timestamp_format: None,
            total_lines: 0,
            level_counts: LevelCounts::default(),
        }
    }

    pub fn merge_chunk(&mut self, chunk: IndexChunk) {
        for &level in &chunk.levels {
            match level {
                LogLevel::Error => self.level_counts.error += 1,
                LogLevel::Warn => self.level_counts.warn += 1,
                LogLevel::Info => self.level_counts.info += 1,
                LogLevel::Debug => self.level_counts.debug += 1,
                LogLevel::Trace => self.level_counts.trace += 1,
                LogLevel::Unknown => {}
            }
        }
        self.line_offsets.extend(chunk.line_offsets);
        self.timestamps.extend(chunk.timestamps);
        self.levels.extend(chunk.levels);
        self.is_entry_start.extend(chunk.is_entry_start);
        self.total_lines = self.line_offsets.len();
    }

    pub fn level_counts(&self) -> &LevelCounts {
        &self.level_counts
    }
}

#[derive(Default, Debug, Clone)]
pub struct LevelCounts {
    pub error: usize,
    pub warn: usize,
    pub info: usize,
    pub debug: usize,
    pub trace: usize,
}

pub fn build_index_chunk(
    data: &[u8],
    start_byte: u64,
    max_lines: usize,
    ts_format: &Option<TimestampFormat>,
) -> IndexChunk {
    let estimated = if max_lines == usize::MAX {
        data.len() / 80 // rough estimate: ~80 bytes per line
    } else {
        max_lines
    };
    let mut offsets = Vec::with_capacity(estimated.min(1_000_000));
    let mut timestamps = Vec::with_capacity(estimated.min(1_000_000));
    let mut levels = Vec::with_capacity(estimated.min(1_000_000));
    let mut is_entry_start = Vec::with_capacity(estimated.min(1_000_000));

    let mut pos = start_byte as usize;
    let mut last_ts: Option<i64> = None;

    if pos == 0 {
        offsets.push(0);
    } else if pos > 0 && pos <= data.len() && (pos == 0 || data[pos - 1] == b'\n') {
        // New data starts right after a newline — register it as a line start
        offsets.push(pos as u64);
    }

    while pos < data.len() && offsets.len() < max_lines.saturating_add(1) {
        if data[pos] == b'\n' {
            let next_line_start = (pos + 1) as u64;
            if pos + 1 < data.len() {
                offsets.push(next_line_start);
            }
        }
        pos += 1;
    }

    for (i, &offset) in offsets.iter().enumerate() {
        let start = offset as usize;
        let end = if i + 1 < offsets.len() {
            (offsets[i + 1] as usize).saturating_sub(1)
        } else {
            data.len()
        };
        let end = end.min(data.len());
        let line_end = (start + 200).min(end);
        let line_slice = &data[start..line_end];

        let line_str = std::str::from_utf8(line_slice).unwrap_or_default();

        let level = detect_level(line_str);
        levels.push(level);

        if let Some(fmt) = ts_format {
            if let Some(ms) = fmt.parse_epoch_ms(line_str) {
                timestamps.push(Some(ms));
                is_entry_start.push(true);
                last_ts = Some(ms);
            } else {
                timestamps.push(last_ts);
                is_entry_start.push(false);
            }
        } else {
            timestamps.push(None);
            is_entry_start.push(true);
        }
    }

    IndexChunk {
        line_offsets: offsets,
        timestamps,
        levels,
        is_entry_start,
    }
}

pub fn detect_timestamp_format(data: &[u8]) -> Option<TimestampFormat> {
    let mut sample_lines = Vec::new();
    let mut pos = 0;
    let mut line_start = 0;

    while pos < data.len() && sample_lines.len() < 20 {
        if data[pos] == b'\n' {
            let end = pos.min(line_start + 200);
            if let Ok(s) = std::str::from_utf8(&data[line_start..end]) {
                sample_lines.push(s.to_string());
            }
            line_start = pos + 1;
        }
        pos += 1;
    }
    if line_start < data.len() && sample_lines.len() < 20 {
        let end = data.len().min(line_start + 200);
        if let Ok(s) = std::str::from_utf8(&data[line_start..end]) {
            sample_lines.push(s.to_string());
        }
    }

    let refs: Vec<&str> = sample_lines.iter().map(|s| s.as_str()).collect();
    TimestampFormat::detect(&refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_chunk_basic() {
        let data = b"2024-01-15 14:32:01 INFO Starting\n2024-01-15 14:32:02 ERROR Failed\n";
        let ts_fmt = detect_timestamp_format(data);
        let chunk = build_index_chunk(data, 0, 1000, &ts_fmt);

        assert_eq!(chunk.line_offsets.len(), 2);
        assert_eq!(chunk.levels[0], LogLevel::Info);
        assert_eq!(chunk.levels[1], LogLevel::Error);
    }

    #[test]
    fn test_continuation_lines() {
        let data = b"2024-01-15 14:32:01 ERROR NullPointer\n    at com.example.Main\n    at java.lang.Thread\n2024-01-15 14:32:02 INFO Next\n";
        let ts_fmt = detect_timestamp_format(data);
        let chunk = build_index_chunk(data, 0, 1000, &ts_fmt);

        assert_eq!(chunk.line_offsets.len(), 4);
        assert!(chunk.is_entry_start[0]);
        assert!(!chunk.is_entry_start[1]);
        assert!(!chunk.is_entry_start[2]);
        assert!(chunk.is_entry_start[3]);
    }
}
