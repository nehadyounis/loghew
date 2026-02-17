use anyhow::{Context, Result};
use memmap2::Mmap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::index::{build_index_chunk, detect_timestamp_format, LogIndex};

const MMAP_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

pub enum LogSource {
    Mmap { mmap: Mmap, index: LogIndex },
    Buffered { content: Vec<u8>, index: LogIndex },
}

impl LogSource {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
        let metadata = file.metadata()?;
        let size = metadata.len();

        if size > MMAP_THRESHOLD {
            let mmap = unsafe { Mmap::map(&file)? };
            let ts_format = detect_timestamp_format(&mmap);
            let mut index = LogIndex::new();
            index.timestamp_format = ts_format.clone();

            // Skip timestamps for large files — parsed incrementally after UI opens
            let chunk = build_index_chunk(&mmap, 0, usize::MAX, &ts_format, true);
            index.merge_chunk(chunk);

            Ok(LogSource::Mmap { mmap, index })
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
            LogSource::Mmap { mmap, index } => {
                *mmap = unsafe { Mmap::map(&file)? };
                // Always skip timestamps in reload — batch parser handles them
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
            LogSource::Mmap { mmap, index } => {
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
}
