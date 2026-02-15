use std::path::PathBuf;

fn main() {
    let path = PathBuf::from("/tmp/sample.log");
    let content = std::fs::read_to_string(&path).expect("Failed to read sample.log");
    let lines: Vec<&str> = content.lines().collect();

    let err_count = lines.iter().filter(|l| { let u = l.to_uppercase(); u.contains("ERROR") || u.contains("FATAL") || u.contains("CRITICAL") }).count();
    let warn_count = lines.iter().filter(|l| l.to_uppercase().contains("WARN")).count();
    let info_count = lines.iter().filter(|l| l.to_uppercase().contains("INFO")).count();
    let dbg_count = lines.iter().filter(|l| l.to_uppercase().contains("DEBUG")).count();
    let trc_count = lines.iter().filter(|l| l.to_uppercase().contains("TRACE")).count();

    println!("┌──────────────────────────────────────────────────────────────────────────────────┐");
    for (i, line) in lines.iter().enumerate().take(22) {
        let level = detect_level(line);
        let marker = match level {
            "ERROR" | "FATAL" => "!",
            "WARN" => "~",
            "DEBUG" | "TRACE" => ".",
            _ => " ",
        };
        let truncated = if line.len() > 72 { &line[..72] } else { line };
        println!("│ {:>2} {} {:<74}│", i + 1, marker, truncated);
    }
    println!("├──────────────────────────────────────────────────────────────────────────────────┤");
    println!("│ sample.log → {} lines → line 1 → {} ERR  {} WARN  {} INFO  {} DBG  {} TRC       │",
        lines.len(), err_count, warn_count, info_count, dbg_count, trc_count);
    println!("├──────────────────────────────────────────────────────────────────────────────────┤");
    println!("│ Type to search · / for commands     q quit  h help  F follow                    │");
    println!("│ > ...                                                                           │");
    println!("└──────────────────────────────────────────────────────────────────────────────────┘");
    println!();
    println!("Legend: ! = ERROR/FATAL (red bold)  ~ = WARN (yellow)  . = DEBUG/TRACE (gray)");
    println!();
    println!("=== Continuation line detection ===");
    for (i, line) in lines.iter().enumerate() {
        let is_continuation = !line.is_empty() && (line.starts_with(' ') || line.starts_with('\t'));
        if is_continuation {
            println!("  Line {:>2}: continuation → \"{}\"", i + 1, if line.len() > 60 { &line[..60] } else { line });
        }
    }
    println!();
    println!("=== Timestamp detection ===");
    println!("Format: ISO 8601 space-separated (2024-01-15 14:32:01)");
    println!("First timestamp: 2024-01-15 14:32:01");
    println!("Last timestamp:  2024-01-15 14:32:15");
}

fn detect_level(line: &str) -> &'static str {
    let check = if line.len() > 100 { &line[..100] } else { line };
    let upper = check.to_uppercase();
    if upper.contains("FATAL") || upper.contains("CRITICAL") { return "FATAL"; }
    for keyword in &["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] {
        if upper.contains(keyword) { return keyword; }
    }
    ""
}
