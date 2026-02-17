use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct TimestampFormat {
    pub regex: Regex,
    pub kind: TimestampKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampKind {
    Iso8601,
    Iso8601Space,
    Syslog,
    Apache,
    UnixEpoch,
    SlashDate,
}

const TIMESTAMP_PATTERNS: &[(TimestampKind, &str)] = &[
    (
        TimestampKind::Iso8601,
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}",
    ),
    (
        TimestampKind::Iso8601Space,
        r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}",
    ),
    (
        TimestampKind::Syslog,
        r"[A-Z][a-z]{2}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}",
    ),
    (
        TimestampKind::Apache,
        r"\d{2}/[A-Z][a-z]{2}/\d{4}:\d{2}:\d{2}:\d{2}",
    ),
    (TimestampKind::UnixEpoch, r"\b\d{10}(?:\.\d{1,6})?\b"),
    (
        TimestampKind::SlashDate,
        r"\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2}",
    ),
];

impl TimestampFormat {
    pub fn detect(lines: &[&str]) -> Option<Self> {
        let mut best_kind = None;
        let mut best_count = 0;

        for &(kind, pattern) in TIMESTAMP_PATTERNS {
            let re = Regex::new(pattern).ok()?;
            let count = lines.iter().filter(|l| re.is_match(l)).count();
            if count > best_count {
                best_count = count;
                best_kind = Some((kind, re));
            }
        }

        if best_count == 0 {
            return None;
        }

        let (kind, regex) = best_kind?;
        Some(TimestampFormat { regex, kind })
    }

    pub fn parse_epoch_ms(&self, line: &str) -> Option<i64> {
        let m = self.regex.find(line)?;
        let s = m.as_str();

        match self.kind {
            TimestampKind::Iso8601 => {
                let dt = chrono::NaiveDateTime::parse_from_str(
                    &s[..19],
                    "%Y-%m-%dT%H:%M:%S",
                )
                .ok()?;
                Some(dt.and_utc().timestamp_millis())
            }
            TimestampKind::Iso8601Space => {
                let dt = chrono::NaiveDateTime::parse_from_str(
                    &s[..19],
                    "%Y-%m-%d %H:%M:%S",
                )
                .ok()?;
                Some(dt.and_utc().timestamp_millis())
            }
            TimestampKind::Syslog => {
                let now = chrono::Utc::now();
                let with_year = format!("{} {}", now.format("%Y"), s);
                let dt = chrono::NaiveDateTime::parse_from_str(
                    &with_year,
                    "%Y %b %d %H:%M:%S",
                )
                .ok()?;
                Some(dt.and_utc().timestamp_millis())
            }
            TimestampKind::Apache => {
                let dt = chrono::NaiveDateTime::parse_from_str(s, "%d/%b/%Y:%H:%M:%S").ok()?;
                Some(dt.and_utc().timestamp_millis())
            }
            TimestampKind::UnixEpoch => {
                let f: f64 = s.parse().ok()?;
                Some((f * 1000.0) as i64)
            }
            TimestampKind::SlashDate => {
                let dt = chrono::NaiveDateTime::parse_from_str(
                    &s[..19],
                    "%Y/%m/%d %H:%M:%S",
                )
                .ok()?;
                Some(dt.and_utc().timestamp_millis())
            }
        }
    }
}

pub static LEVEL_REGEX: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"(?i)\b(ERROR|FATAL|CRITICAL|WARN|WARNING|INFO|DEBUG|TRACE)\b").unwrap());

pub fn detect_level(line: &str) -> LogLevel {
    let end = if line.len() > 100 {
        let mut i = 100;
        while !line.is_char_boundary(i) {
            i -= 1;
        }
        i
    } else {
        line.len()
    };
    let check = &line[..end];

    if let Some(m) = LEVEL_REGEX.find(check) {
        match m.as_str().to_ascii_uppercase().as_str() {
            "ERROR" | "FATAL" | "CRITICAL" => LogLevel::Error,
            "WARN" | "WARNING" => LogLevel::Warn,
            "INFO" => LogLevel::Info,
            "DEBUG" => LogLevel::Debug,
            "TRACE" => LogLevel::Trace,
            _ => LogLevel::Unknown,
        }
    } else {
        LogLevel::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_detection() {
        assert_eq!(detect_level("2024-01-15 ERROR something"), LogLevel::Error);
        assert_eq!(detect_level("2024-01-15 error something"), LogLevel::Error);
        assert_eq!(detect_level("FATAL crash"), LogLevel::Error);
        assert_eq!(detect_level("CRITICAL failure"), LogLevel::Error);
        assert_eq!(detect_level("[WARN] slow query"), LogLevel::Warn);
        assert_eq!(detect_level("WARNING: disk full"), LogLevel::Warn);
        assert_eq!(detect_level("INFO starting"), LogLevel::Info);
        assert_eq!(detect_level("DEBUG x=42"), LogLevel::Debug);
        assert_eq!(detect_level("TRACE enter fn"), LogLevel::Trace);
        assert_eq!(detect_level("no level here"), LogLevel::Unknown);
    }

    #[test]
    fn test_level_whole_word_boundary() {
        assert_eq!(detect_level("INFORMATION about"), LogLevel::Unknown);
        assert_eq!(detect_level("DEBUGGING session"), LogLevel::Unknown);
    }

    #[test]
    fn test_timestamp_detection_iso8601() {
        let lines = vec![
            "2024-01-15T14:32:01.003Z INFO Starting",
            "2024-01-15T14:32:02.000Z ERROR Failed",
        ];
        let fmt = TimestampFormat::detect(&lines).unwrap();
        assert_eq!(fmt.kind, TimestampKind::Iso8601);
        assert!(fmt.parse_epoch_ms(lines[0]).is_some());
    }

    #[test]
    fn test_timestamp_detection_iso8601_space() {
        let lines = vec![
            "2024-01-15 14:32:01,003 INFO Starting",
            "2024-01-15 14:32:02,000 ERROR Failed",
        ];
        let fmt = TimestampFormat::detect(&lines).unwrap();
        assert_eq!(fmt.kind, TimestampKind::Iso8601Space);
    }

    #[test]
    fn test_timestamp_detection_syslog() {
        let lines = vec![
            "Jan 15 14:32:01 myhost app: starting",
            "Jan 15 14:32:02 myhost app: error",
        ];
        let fmt = TimestampFormat::detect(&lines).unwrap();
        assert_eq!(fmt.kind, TimestampKind::Syslog);
    }

    #[test]
    fn test_timestamp_detection_unix_epoch() {
        let lines = vec![
            "1705312321.003 INFO starting",
            "1705312322.000 ERROR failed",
        ];
        let fmt = TimestampFormat::detect(&lines).unwrap();
        assert_eq!(fmt.kind, TimestampKind::UnixEpoch);
    }
}
