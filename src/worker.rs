use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use memmap2::Mmap;
use regex::Regex;

use crate::log::{
    IndexChunk, LevelCounts, LogLevel, TimestampFormat, detect_level,
};
use crate::log::index::build_index_chunk;

pub type Generation = u64;

pub enum WorkRequest {
    ScanBatch {
        mmap: Arc<Mmap>,
        start_byte: u64,
        scan_limit: u64,
        ts_format: Option<TimestampFormat>,
        max_lines: usize,
    },
    DeferredParseBatch {
        mmap: Arc<Mmap>,
        offsets: Vec<u64>,
        start_index: usize,
        ts_format: Option<TimestampFormat>,
        last_ts: Option<i64>,
    },
    SearchBatch {
        mmap: Arc<Mmap>,
        regex: Regex,
        line_offsets: Arc<Vec<u64>>,
        data_len: usize,
        start_line: usize,
        batch_size: usize,
        generation: Generation,
        cancel: Arc<AtomicBool>,
    },
    FilterBatch {
        mmap: Arc<Mmap>,
        conditions: Vec<(Regex, bool)>,
        line_offsets: Arc<Vec<u64>>,
        data_len: usize,
        start_line: usize,
        batch_size: usize,
        generation: Generation,
        cancel: Arc<AtomicBool>,
    },
    Quit,
}

pub enum WorkResult {
    ScanChunk {
        chunk: IndexChunk,
        next_offset: u64,
    },
    DeferredParsed {
        start_index: usize,
        timestamps: Vec<Option<i64>>,
        levels: Vec<LogLevel>,
        is_entry_start: Vec<bool>,
        level_counts_delta: LevelCounts,
        last_parsed_ts: Option<i64>,
    },
    SearchBatch {
        matches: Vec<usize>,
        cursor: usize,
        done: bool,
        generation: Generation,
    },
    FilterBatch {
        matches: Vec<usize>,
        cursor: usize,
        done: bool,
        generation: Generation,
    },
}

pub fn worker_loop(rx: Receiver<WorkRequest>, tx: Sender<WorkResult>) {
    while let Ok(req) = rx.recv() {
        match req {
            WorkRequest::Quit => break,
            WorkRequest::ScanBatch { mmap, start_byte, scan_limit, ts_format, max_lines } => {
                let limit = (scan_limit as usize).min(mmap.len());
                let chunk = build_index_chunk(&mmap[..limit], start_byte, max_lines, &ts_format, true);
                let next_offset = if chunk.line_offsets.is_empty() {
                    scan_limit
                } else {
                    chunk.line_offsets.last().map(|&x| x + 1).unwrap_or(scan_limit)
                };
                let _ = tx.send(WorkResult::ScanChunk { chunk, next_offset });
            }
            WorkRequest::DeferredParseBatch { mmap, offsets, start_index, ts_format, last_ts } => {
                let data: &[u8] = &mmap;
                let result = parse_deferred_range(data, &offsets, &ts_format, last_ts);
                let _ = tx.send(WorkResult::DeferredParsed {
                    start_index,
                    timestamps: result.timestamps,
                    levels: result.levels,
                    is_entry_start: result.is_entry_start,
                    level_counts_delta: result.level_counts_delta,
                    last_parsed_ts: result.last_parsed_ts,
                });
            }
            WorkRequest::SearchBatch { mmap, regex, line_offsets, data_len, start_line, batch_size, generation, cancel } => {
                let data: &[u8] = &mmap;
                let total = line_offsets.len();
                let end = (start_line + batch_size).min(total);
                let mut matches = Vec::new();
                let mut cursor = end;
                let mut cancelled = false;
                for i in start_line..end {
                    if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
                        cursor = i;
                        cancelled = true;
                        break;
                    }
                    if let Some(line) = get_line_from_data(data, &line_offsets, data_len, i) {
                        if regex.is_match(line) {
                            matches.push(i);
                        }
                    }
                }
                let done = cursor >= total || cancelled;
                let _ = tx.send(WorkResult::SearchBatch { matches, cursor, done, generation });
            }
            WorkRequest::FilterBatch { mmap, conditions, line_offsets, data_len, start_line, batch_size, generation, cancel } => {
                let data: &[u8] = &mmap;
                let total = line_offsets.len();
                let end = (start_line + batch_size).min(total);
                let mut matches = Vec::new();
                let mut cursor = end;
                let mut cancelled = false;
                for i in start_line..end {
                    if i % 1024 == 0 && cancel.load(Ordering::Relaxed) {
                        cursor = i;
                        cancelled = true;
                        break;
                    }
                    if let Some(line) = get_line_from_data(data, &line_offsets, data_len, i) {
                        let pass = conditions.iter().all(|(re, negated)| {
                            let found = re.is_match(line);
                            if *negated { !found } else { found }
                        });
                        if pass {
                            matches.push(i);
                        }
                    }
                }
                let done = cursor >= total || cancelled;
                let _ = tx.send(WorkResult::FilterBatch { matches, cursor, done, generation });
            }
        }
    }
}

fn get_line_from_data<'a>(data: &'a [u8], offsets: &[u64], data_len: usize, i: usize) -> Option<&'a str> {
    if i >= offsets.len() {
        return None;
    }
    let start = offsets[i] as usize;
    let end = if i + 1 < offsets.len() {
        (offsets[i + 1] as usize).saturating_sub(1)
    } else {
        data_len
    };
    let end = end.min(data_len);
    if start >= data_len {
        return None;
    }
    let slice = &data[start..end];
    let truncated = if slice.len() > 2000 { &slice[..2000] } else { slice };
    std::str::from_utf8(truncated)
        .ok()
        .map(|s| s.trim_end_matches(['\r', '\n']))
}

struct DeferredParseResult {
    timestamps: Vec<Option<i64>>,
    levels: Vec<LogLevel>,
    is_entry_start: Vec<bool>,
    level_counts_delta: LevelCounts,
    last_parsed_ts: Option<i64>,
}

fn parse_deferred_range(
    data: &[u8],
    offsets: &[u64],
    ts_format: &Option<TimestampFormat>,
    mut last_ts: Option<i64>,
) -> DeferredParseResult {
    let n = offsets.len();
    let mut timestamps = Vec::with_capacity(n);
    let mut levels = Vec::with_capacity(n);
    let mut is_entry_start = Vec::with_capacity(n);
    let mut counts = LevelCounts::default();

    for (i, &offset) in offsets.iter().enumerate() {
        let line_start = offset as usize;
        if line_start >= data.len() {
            timestamps.push(None);
            levels.push(LogLevel::Unknown);
            is_entry_start.push(true);
            continue;
        }
        let line_end = if i + 1 < n {
            (offsets[i + 1] as usize).saturating_sub(1)
        } else {
            data.len()
        };
        let line_end = line_end.min(data.len());
        let check_end = (line_start + 200).min(line_end);
        let line_slice = &data[line_start..check_end];
        let line_str = std::str::from_utf8(line_slice).unwrap_or_default();

        let level = detect_level(line_str);
        match level {
            LogLevel::Error => counts.error += 1,
            LogLevel::Warn => counts.warn += 1,
            LogLevel::Info => counts.info += 1,
            LogLevel::Debug => counts.debug += 1,
            LogLevel::Trace => counts.trace += 1,
            LogLevel::Unknown => {}
        }
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

    DeferredParseResult {
        timestamps,
        levels,
        is_entry_start,
        level_counts_delta: counts,
        last_parsed_ts: last_ts,
    }
}
