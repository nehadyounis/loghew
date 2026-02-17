pub(crate) mod index;
mod parser;
mod reader;

pub use index::{IndexChunk, LevelCounts};
pub use parser::{detect_level, LogLevel, TimestampFormat, LEVEL_REGEX};
pub use reader::{LogSource, read_file_tail};
