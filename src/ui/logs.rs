use chrono::{DateTime, Utc};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// LogLevel — detected from log line text
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

static LOG_LEVEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(trace|debug|info|warn(?:ing)?|error)\b"#).unwrap());

/// Detect log level from a line of text.
pub fn detect_log_level(text: &str) -> Option<LogLevel> {
    LOG_LEVEL_RE.find(text).and_then(|m| {
        let s = m.as_str().to_lowercase();
        match s.as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    })
}

// ---------------------------------------------------------------------------
// LogLine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub timestamp: DateTime<Utc>,
    pub service: String,
    pub text: String,
    pub is_stderr: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
}

// ---------------------------------------------------------------------------
// LogWriter — colored terminal output with owo-colors
// ---------------------------------------------------------------------------

pub struct LogWriter {
    rx: mpsc::Receiver<LogLine>,
    max_name_len: usize,
    use_color: bool,
}

/// Color palette for service names (using owo-colors).
const SERVICE_COLORS: &[fn(&str) -> String] = &[
    |s| format!("{}", s.cyan()),
    |s| format!("{}", s.yellow()),
    |s| format!("{}", s.green()),
    |s| format!("{}", s.magenta()),
    |s| format!("{}", s.blue()),
    |s| format!("{}", s.red()),
];

fn format_level(level: &LogLevel, use_color: bool) -> String {
    if !use_color {
        return format!("{:>5} ", level.as_str());
    }
    match level {
        LogLevel::Trace => format!("{} ", level.as_str().dimmed()),
        LogLevel::Debug => format!("{} ", level.as_str().blue()),
        LogLevel::Info => format!("{} ", level.as_str().green()),
        LogLevel::Warn => format!("{} ", level.as_str().yellow()),
        LogLevel::Error => format!("{} ", level.as_str().red()),
    }
}

impl LogWriter {
    pub fn new(rx: mpsc::Receiver<LogLine>, max_name_len: usize) -> Self {
        Self {
            rx,
            max_name_len,
            use_color: std::io::stdout().is_terminal(),
        }
    }

    pub async fn run(mut self) {
        let mut color_map: BTreeMap<String, usize> = BTreeMap::new();
        let mut next_color = 0usize;

        while let Some(line) = self.rx.recv().await {
            let color_idx = *color_map.entry(line.service.clone()).or_insert_with(|| {
                let idx = next_color;
                next_color = (next_color + 1) % SERVICE_COLORS.len();
                idx
            });

            // Build the output line in a String buffer, then print atomically.
            // This avoids holding a StdoutLock across the await boundary.
            let mut buf = String::new();

            // Service name (colored)
            if self.use_color {
                let colored_name = SERVICE_COLORS[color_idx](&line.service);
                let padding = self.max_name_len.saturating_sub(line.service.len());
                for _ in 0..padding {
                    buf.push(' ');
                }
                buf.push_str(&colored_name);
                buf.push_str(&format!(" {} ", "|".dimmed()));
            } else {
                buf.push_str(&format!(
                    "{:>width$} | ",
                    line.service,
                    width = self.max_name_len,
                ));
            }

            // Log level (colored)
            if let Some(ref level) = line.level {
                buf.push_str(&format_level(level, self.use_color));
            }

            // Log text
            if self.use_color && line.is_stderr {
                buf.push_str(&format!("{}", line.text.red()));
            } else {
                buf.push_str(&line.text);
            }

            println!("{}", buf);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_level_info() {
        assert_eq!(detect_log_level("[INFO] starting"), Some(LogLevel::Info));
        assert_eq!(detect_log_level("level=info msg=ok"), Some(LogLevel::Info));
    }

    #[test]
    fn detect_level_error() {
        assert_eq!(
            detect_log_level("ERROR: something failed"),
            Some(LogLevel::Error)
        );
        assert_eq!(
            detect_log_level(r#"{"level":"error","msg":"fail"}"#),
            Some(LogLevel::Error)
        );
    }

    #[test]
    fn detect_level_warn() {
        assert_eq!(detect_log_level("[WARN] slow query"), Some(LogLevel::Warn));
        assert_eq!(
            detect_log_level("WARNING: deprecated"),
            Some(LogLevel::Warn)
        );
    }

    #[test]
    fn detect_level_debug() {
        assert_eq!(
            detect_log_level("DEBUG: detailed info"),
            Some(LogLevel::Debug)
        );
    }

    #[test]
    fn detect_level_trace() {
        assert_eq!(
            detect_log_level("TRACE entering function"),
            Some(LogLevel::Trace)
        );
    }

    #[test]
    fn detect_level_none() {
        assert_eq!(detect_log_level("just a plain message"), None);
        assert_eq!(detect_log_level(""), None);
    }

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn log_line_serialization() {
        let line = LogLine {
            timestamp: Utc::now(),
            service: "api".to_string(),
            text: "hello world".to_string(),
            is_stderr: false,
            level: Some(LogLevel::Info),
        };
        let json = serde_json::to_string(&line).unwrap();
        assert!(json.contains("\"service\":\"api\""));
        assert!(json.contains("\"level\":\"info\""));

        let deserialized: LogLine = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.service, "api");
        assert_eq!(deserialized.level, Some(LogLevel::Info));
    }
}
