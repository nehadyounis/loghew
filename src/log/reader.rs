use anyhow::{Context, Result};
use memmap2::Mmap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use super::index::{build_index_chunk, detect_timestamp_format, LogIndex};

const MMAP_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

pub enum LogSource {
    Mmap { mmap: Arc<Mmap>, index: LogIndex, scan_offset: u64, scan_limit: u64 },
    Buffered { content: Vec<u8>, index: LogIndex },
}

impl LogSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let metadata = file.metadata()?;
        let size = metadata.len();

        if size > MMAP_THRESHOLD {
            let mmap = Arc::new(unsafe { Mmap::map(&file)? });
            let ts_format = detect_timestamp_format(&mmap);
            let mut index = LogIndex::new();
            index.timestamp_format = ts_format.clone();

            let chunk = build_index_chunk(&mmap, 0, 50_000, &ts_format, true);
            let scan_offset = chunk.line_offsets.last().map(|&x| x + 1).unwrap_or(size);
            index.merge_chunk(chunk);

            Ok(LogSource::Mmap { mmap, index, scan_offset, scan_limit: size })
        } else {
            let content = std::fs::read(path)?;
            let ts_format = detect_timestamp_format(&content);
            let mut index = LogIndex::new();
            index.timestamp_format = ts_format.clone();

            let chunk = build_index_chunk(&content, 0, usize::MAX, &ts_format, false);
            index.merge_chunk(chunk);

            Ok(LogSource::Buffered { content, index })
        }
    }

    pub fn open_stdin() -> Result<Self> {
        use std::io::Read;
        let mut content = Vec::new();
        std::io::stdin().read_to_end(&mut content)?;

        let ts_format = detect_timestamp_format(&content);
        let mut index = LogIndex::new();
        index.timestamp_format = ts_format.clone();

        let chunk = build_index_chunk(&content, 0, usize::MAX, &ts_format, false);
        index.merge_chunk(chunk);

        Ok(LogSource::Buffered { content, index })
    }

    pub fn reload(&mut self, path: &Path) -> Result<bool> {
        let file = File::open(path)?;
        let new_size = file.metadata()?.len();
        let old_size = self.data().len() as u64;

        if new_size <= old_size {
            return Ok(false);
        }

        let ts_format = self.index().timestamp_format.clone();

        match self {
            LogSource::Mmap { mmap, index, .. } => {
                *mmap = Arc::new(unsafe { Mmap::map(&file)? });
                let chunk = build_index_chunk(mmap, old_size, usize::MAX, &ts_format, true);
                if chunk.line_offsets.is_empty() {
                    return Ok(false);
                }
                index.merge_chunk(chunk);
            }
            LogSource::Buffered { content, index } => {
                let mut f = file;
                f.seek(SeekFrom::Start(old_size))?;
                let mut new_bytes = Vec::new();
                f.read_to_end(&mut new_bytes)?;
                if new_bytes.is_empty() {
                    return Ok(false);
                }
                let base_offset = content.len() as u64;
                content.extend_from_slice(&new_bytes);
                let chunk = build_index_chunk(content, base_offset, usize::MAX, &ts_format, true);
                if chunk.line_offsets.is_empty() {
                    return Ok(false);
                }
                index.merge_chunk(chunk);
            }
        }

        Ok(true)
    }

    pub fn mmap_arc(&self) -> Option<Arc<Mmap>> {
        match self {
            LogSource::Mmap { mmap, .. } => Some(Arc::clone(mmap)),
            LogSource::Buffered { .. } => None,
        }
    }

    pub fn is_mmap(&self) -> bool {
        matches!(self, LogSource::Mmap { .. })
    }

    pub fn data(&self) -> &[u8] {
        match self {
            LogSource::Mmap { mmap, .. } => mmap,
            LogSource::Buffered { content, .. } => content,
        }
    }

    pub fn index(&self) -> &LogIndex {
        match self {
            LogSource::Mmap { index, .. } => index,
            LogSource::Buffered { index, .. } => index,
        }
    }

    pub fn index_mut(&mut self) -> &mut LogIndex {
        match self {
            LogSource::Mmap { index, .. } => index,
            LogSource::Buffered { index, .. } => index,
        }
    }

    pub fn get_line(&self, line_num: usize) -> Option<&str> {
        let index = self.index();
        if line_num >= index.line_offsets.len() {
            return None;
        }

        let data = self.data();
        let start = index.line_offsets[line_num] as usize;
        let end = if line_num + 1 < index.line_offsets.len() {
            (index.line_offsets[line_num + 1] as usize).saturating_sub(1)
        } else {
            data.len()
        };

        let end = end.min(data.len());
        if start >= data.len() {
            return None;
        }

        let slice = &data[start..end];
        let truncated = if slice.len() > 2000 {
            &slice[..2000]
        } else {
            slice
        };

        std::str::from_utf8(truncated)
            .ok()
            .map(|s| s.trim_end_matches(['\r', '\n']))
    }

    pub fn parse_deferred_batch(&mut self, batch_size: usize) -> bool {
        match self {
            LogSource::Mmap { mmap, index, .. } => {
                index.parse_deferred_batch(mmap, batch_size)
            }
            LogSource::Buffered { content, index } => {
                index.parse_deferred_batch(content, batch_size)
            }
        }
    }

    pub fn indexing_ready(&self) -> bool {
        let idx = self.index();
        idx.timestamps_ready && idx.levels_ready
    }

    pub fn scanning(&self) -> bool {
        match self {
            LogSource::Mmap { scan_offset, scan_limit, .. } => *scan_offset < *scan_limit,
            LogSource::Buffered { .. } => false,
        }
    }

    pub fn scan_batch(&mut self) -> bool {
        match self {
            LogSource::Mmap { mmap, index, scan_offset, scan_limit } => {
                if *scan_offset >= *scan_limit {
                    return false;
                }
                let limit = (*scan_limit as usize).min(mmap.len());
                let ts_fmt = index.timestamp_format.clone();
                let chunk = build_index_chunk(
                    &mmap[..limit],
                    *scan_offset,
                    50_000,
                    &ts_fmt,
                    true,
                );
                if chunk.line_offsets.is_empty() {
                    *scan_offset = *scan_limit;
                    return false;
                }
                *scan_offset = chunk.line_offsets.last().map(|&x| x + 1).unwrap_or(*scan_limit);
                index.merge_chunk(chunk);
                *scan_offset < *scan_limit
            }
            LogSource::Buffered { .. } => false,
        }
    }

    pub fn scan_progress(&self) -> Option<(u64, u64)> {
        match self {
            LogSource::Mmap { scan_offset, scan_limit, .. } if *scan_offset < *scan_limit => {
                Some((*scan_offset, *scan_limit))
            }
            _ => None,
        }
    }

    pub fn scan_tail(&self, count: usize) -> Vec<(String, super::parser::LogLevel)> {
        let data = self.data();
        if data.is_empty() {
            return Vec::new();
        }
        let mut newline_count = 0;
        let mut start = data.len();
        for i in (0..data.len()).rev() {
            if data[i] == b'\n' {
                newline_count += 1;
                if newline_count >= count + 1 {
                    start = i + 1;
                    break;
                }
            }
        }
        if newline_count < count + 1 {
            start = 0;
        }
        let tail_data = &data[start..];
        String::from_utf8_lossy(tail_data)
            .lines()
            .map(|line| {
                let level = super::parser::detect_level(line);
                (line.to_string(), level)
            })
            .collect()
    }
}

/// Read the last count lines fresh from disk (not mmap).
/// Catches file growth that the mmap snapshot doesn't cover.
pub fn read_file_tail(path: &Path, count: usize) -> Vec<(String, super::parser::LogLevel)> {
    use std::io::{Read as _, Seek, SeekFrom};

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        return Vec::new();
    }

    let buf_size: u64 = 128 * 1024;
    let start = size.saturating_sub(buf_size);
    let _ = file.seek(SeekFrom::Start(start));

    let mut buf = Vec::with_capacity(buf_size as usize);
    let _ = file.read_to_end(&mut buf);

    let text = String::from_utf8_lossy(&buf);
    let all_lines: Vec<&str> = text.lines().collect();
    let start_idx = all_lines.len().saturating_sub(count);
    all_lines[start_idx..]
        .iter()
        .map(|line| {
            let level = super::parser::detect_level(line);
            (line.to_string(), level)
        })
        .collect()
}
