use anyhow::{bail, Result};
use chrono::{Duration, Utc};
use regex::Regex;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::config::resolve::resolve_config;
use crate::ui::filter::LogFilter;
use crate::ui::logs::{LogLevel, LogLine};

/// Parse a human-readable duration string like "5m", "1h", "30s".
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration string");
    }

    let (num_str, unit) = if let Some(stripped) = s.strip_suffix("ms") {
        (stripped, "ms")
    } else {
        let split = s.len() - 1;
        (&s[..split], &s[split..])
    };

    let num: i64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration number: {}", num_str))?;

    match unit {
        "s" => Ok(Duration::seconds(num)),
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        "ms" => Ok(Duration::milliseconds(num)),
        _ => bail!("unknown duration unit '{}' (use s, m, h, d, ms)", unit),
    }
}

fn parse_level(s: &str) -> Result<LogLevel> {
    match s.to_lowercase().as_str() {
        "trace" => Ok(LogLevel::Trace),
        "debug" => Ok(LogLevel::Debug),
        "info" => Ok(LogLevel::Info),
        "warn" | "warning" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        _ => bail!(
            "unknown log level '{}' (use trace, debug, info, warn, error)",
            s
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    config_file: Option<&Path>,
    services: Vec<String>,
    tail: Option<usize>,
    since: Option<String>,
    grep: Option<String>,
    exclude: Option<String>,
    level: Option<String>,
    format: String,
    output: Option<PathBuf>,
    timestamps: bool,
) -> Result<()> {
    let config_path = resolve_config(config_file)?;
    let state_dir = config_path
        .parent()
        .expect("config file must have a parent directory")
        .join(".devrig");

    let log_file = state_dir.join("logs").join("current.jsonl");
    if !log_file.exists() {
        bail!(
            "No log file found at {}. Are services running?",
            log_file.display()
        );
    }

    // Build filter
    let mut filter = LogFilter::new();
    if !services.is_empty() {
        filter.services = services;
    }
    if let Some(ref l) = level {
        filter.min_level = Some(parse_level(l)?);
    }
    if let Some(ref g) = grep {
        filter.include =
            Some(Regex::new(g).map_err(|e| anyhow::anyhow!("invalid grep regex: {}", e))?);
    }
    if let Some(ref x) = exclude {
        filter.exclude =
            Some(Regex::new(x).map_err(|e| anyhow::anyhow!("invalid exclude regex: {}", e))?);
    }

    // Parse --since into a cutoff timestamp
    let since_cutoff = since
        .map(|s| parse_duration(&s).map(|d| Utc::now() - d))
        .transpose()?;

    // Read and filter lines from JSONL
    let file = std::fs::File::open(&log_file)?;
    let reader = BufReader::new(file);

    let mut lines: Vec<LogLine> = Vec::new();
    for line_result in reader.lines() {
        let line_str = line_result?;
        if line_str.trim().is_empty() {
            continue;
        }
        let log_line: LogLine = match serde_json::from_str(&line_str) {
            Ok(l) => l,
            Err(_) => continue, // skip malformed lines
        };

        // Apply --since filter
        if let Some(cutoff) = since_cutoff {
            if log_line.timestamp < cutoff {
                continue;
            }
        }

        if filter.matches(&log_line) {
            lines.push(log_line);
        }
    }

    // Apply --tail
    if let Some(n) = tail {
        let skip = lines.len().saturating_sub(n);
        lines = lines.into_iter().skip(skip).collect();
    }

    // Output
    let mut out: Box<dyn Write> = if let Some(ref path) = output {
        Box::new(std::io::BufWriter::new(std::fs::File::create(path)?))
    } else {
        Box::new(std::io::stdout())
    };

    match format.as_str() {
        "json" => {
            for line in &lines {
                serde_json::to_writer(&mut out, line)?;
                writeln!(out)?;
            }
        }
        _ => {
            for line in &lines {
                if timestamps {
                    write!(out, "{} ", line.timestamp.format("%H:%M:%S%.3f"))?;
                }
                if let Some(ref level) = line.level {
                    write!(out, "{:>5} ", level.as_str())?;
                }
                writeln!(out, "{} | {}", line.service, line.text)?;
            }
        }
    }

    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_seconds() {
        let d = parse_duration("30s").unwrap();
        assert_eq!(d, Duration::seconds(30));
    }

    #[test]
    fn parse_duration_minutes() {
        let d = parse_duration("5m").unwrap();
        assert_eq!(d, Duration::minutes(5));
    }

    #[test]
    fn parse_duration_hours() {
        let d = parse_duration("2h").unwrap();
        assert_eq!(d, Duration::hours(2));
    }

    #[test]
    fn parse_duration_milliseconds() {
        let d = parse_duration("500ms").unwrap();
        assert_eq!(d, Duration::milliseconds(500));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("").is_err());
        assert!(parse_duration("5x").is_err());
    }

    #[test]
    fn parse_level_valid() {
        assert_eq!(parse_level("trace").unwrap(), LogLevel::Trace);
        assert_eq!(parse_level("DEBUG").unwrap(), LogLevel::Debug);
        assert_eq!(parse_level("Info").unwrap(), LogLevel::Info);
        assert_eq!(parse_level("warn").unwrap(), LogLevel::Warn);
        assert_eq!(parse_level("WARNING").unwrap(), LogLevel::Warn);
        assert_eq!(parse_level("error").unwrap(), LogLevel::Error);
    }

    #[test]
    fn parse_level_invalid() {
        assert!(parse_level("critical").is_err());
        assert!(parse_level("").is_err());
    }
}
