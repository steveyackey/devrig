use regex::Regex;

use crate::ui::logs::{LogLevel, LogLine};

/// Predicate chain for filtering log lines.
pub struct LogFilter {
    pub services: Vec<String>,
    pub min_level: Option<LogLevel>,
    pub include: Option<Regex>,
    pub exclude: Option<Regex>,
    pub stderr_only: bool,
}

impl LogFilter {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            min_level: None,
            include: None,
            exclude: None,
            stderr_only: false,
        }
    }

    /// Returns true if the log line matches all filter predicates.
    pub fn matches(&self, line: &LogLine) -> bool {
        // Service filter
        if !self.services.is_empty() && !self.services.contains(&line.service) {
            return false;
        }

        // Level filter
        if let Some(min) = &self.min_level {
            if let Some(level) = &line.level {
                if level < min {
                    return false;
                }
            }
        }

        // Include pattern
        if let Some(ref re) = self.include {
            if !re.is_match(&line.text) {
                return false;
            }
        }

        // Exclude pattern
        if let Some(ref re) = self.exclude {
            if re.is_match(&line.text) {
                return false;
            }
        }

        // Stderr only
        if self.stderr_only && !line.is_stderr {
            return false;
        }

        true
    }
}

impl Default for LogFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_line(service: &str, text: &str, level: Option<LogLevel>) -> LogLine {
        LogLine {
            timestamp: Utc::now(),
            service: service.to_string(),
            text: text.to_string(),
            is_stderr: false,
            level,
        }
    }

    #[test]
    fn empty_filter_matches_all() {
        let filter = LogFilter::new();
        assert!(filter.matches(&make_line("api", "hello", None)));
        assert!(filter.matches(&make_line("web", "world", Some(LogLevel::Error))));
    }

    #[test]
    fn service_filter() {
        let filter = LogFilter {
            services: vec!["api".to_string()],
            ..LogFilter::new()
        };
        assert!(filter.matches(&make_line("api", "hello", None)));
        assert!(!filter.matches(&make_line("web", "hello", None)));
    }

    #[test]
    fn level_filter() {
        let filter = LogFilter {
            min_level: Some(LogLevel::Warn),
            ..LogFilter::new()
        };
        assert!(filter.matches(&make_line("api", "error msg", Some(LogLevel::Error))));
        assert!(filter.matches(&make_line("api", "warn msg", Some(LogLevel::Warn))));
        assert!(!filter.matches(&make_line("api", "info msg", Some(LogLevel::Info))));
        // Lines without a level should pass
        assert!(filter.matches(&make_line("api", "plain msg", None)));
    }

    #[test]
    fn regex_include() {
        let filter = LogFilter {
            include: Some(Regex::new("startup").unwrap()),
            ..LogFilter::new()
        };
        assert!(filter.matches(&make_line("api", "startup complete", None)));
        assert!(!filter.matches(&make_line("api", "processing request", None)));
    }

    #[test]
    fn regex_exclude() {
        let filter = LogFilter {
            exclude: Some(Regex::new("health").unwrap()),
            ..LogFilter::new()
        };
        assert!(filter.matches(&make_line("api", "request handled", None)));
        assert!(!filter.matches(&make_line("api", "health check ok", None)));
    }

    #[test]
    fn combined_filters() {
        let filter = LogFilter {
            services: vec!["api".to_string()],
            min_level: Some(LogLevel::Warn),
            include: Some(Regex::new("database").unwrap()),
            ..LogFilter::new()
        };
        // Matches: correct service, high enough level, contains "database"
        assert!(filter.matches(&make_line("api", "database error", Some(LogLevel::Error))));
        // Wrong service
        assert!(!filter.matches(&make_line("web", "database error", Some(LogLevel::Error))));
        // Level too low
        assert!(!filter.matches(&make_line("api", "database info", Some(LogLevel::Info))));
        // Doesn't match pattern
        assert!(!filter.matches(&make_line("api", "cache error", Some(LogLevel::Error))));
    }

    #[test]
    fn stderr_only_filter() {
        let filter = LogFilter {
            stderr_only: true,
            ..LogFilter::new()
        };
        let mut line = make_line("api", "error output", None);
        assert!(!filter.matches(&line));
        line.is_stderr = true;
        assert!(filter.matches(&line));
    }
}
