use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;

use crate::otel::query::{RelatedTelemetry, SystemStatus, TraceSummary};
use crate::otel::types::{LogSeverity, StoredLog, StoredMetric, StoredSpan};

// -----------------------------------------------------------------------
// Output format selection
// -----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Jsonl,
}

impl OutputFormat {
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("json") => OutputFormat::Json,
            Some("jsonl") => OutputFormat::Jsonl,
            _ => OutputFormat::Table,
        }
    }
}

// -----------------------------------------------------------------------
// Trace output
// -----------------------------------------------------------------------

pub fn print_traces(traces: &[TraceSummary], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(traces).unwrap_or_default()
            );
        }
        OutputFormat::Jsonl => {
            for t in traces {
                println!("{}", serde_json::to_string(t).unwrap_or_default());
            }
        }
        OutputFormat::Table => print_traces_table(traces),
    }
}

fn print_traces_table(traces: &[TraceSummary]) {
    if traces.is_empty() {
        println!("  No traces found.");
        return;
    }

    let use_color = std::io::stdout().is_terminal();
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Trace ID").set_alignment(CellAlignment::Left),
        Cell::new("Operation").set_alignment(CellAlignment::Left),
        Cell::new("Services").set_alignment(CellAlignment::Left),
        Cell::new("Duration").set_alignment(CellAlignment::Right),
        Cell::new("Spans").set_alignment(CellAlignment::Right),
        Cell::new("Status").set_alignment(CellAlignment::Center),
    ]);

    for t in traces {
        let short_id = if t.trace_id.len() > 16 {
            &t.trace_id[..16]
        } else {
            &t.trace_id
        };

        let duration = format_duration_ms(t.duration_ms);
        let services = t.services.join(", ");

        let status_text = if use_color {
            if t.has_error {
                format!("{}", "ERROR".red())
            } else {
                format!("{}", "OK".green())
            }
        } else if t.has_error {
            "ERROR".to_string()
        } else {
            "OK".to_string()
        };

        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(&t.root_operation),
            Cell::new(&services),
            Cell::new(&duration),
            Cell::new(t.span_count),
            Cell::new(&status_text),
        ]);
    }

    for line in table.to_string().lines() {
        println!("  {}", line);
    }
}

// -----------------------------------------------------------------------
// Span detail output (for trace detail view)
// -----------------------------------------------------------------------

pub fn print_spans(spans: &[StoredSpan], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(spans).unwrap_or_default()
            );
        }
        OutputFormat::Jsonl => {
            for s in spans {
                println!("{}", serde_json::to_string(s).unwrap_or_default());
            }
        }
        OutputFormat::Table => print_spans_table(spans),
    }
}

fn print_spans_table(spans: &[StoredSpan]) {
    if spans.is_empty() {
        println!("  No spans found.");
        return;
    }

    let use_color = std::io::stdout().is_terminal();
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Span ID").set_alignment(CellAlignment::Left),
        Cell::new("Service").set_alignment(CellAlignment::Left),
        Cell::new("Operation").set_alignment(CellAlignment::Left),
        Cell::new("Duration").set_alignment(CellAlignment::Right),
        Cell::new("Kind").set_alignment(CellAlignment::Center),
        Cell::new("Status").set_alignment(CellAlignment::Center),
    ]);

    for s in spans {
        let short_id = if s.span_id.len() > 16 {
            &s.span_id[..16]
        } else {
            &s.span_id
        };

        let duration = format_duration_ms(s.duration_ms);

        let kind_str = format!("{:?}", s.kind);

        let status_text = if use_color {
            match s.status {
                crate::otel::types::SpanStatus::Error => format!("{}", "ERROR".red()),
                crate::otel::types::SpanStatus::Ok => format!("{}", "OK".green()),
                crate::otel::types::SpanStatus::Unset => "UNSET".to_string(),
            }
        } else {
            format!("{:?}", s.status)
        };

        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(&s.service_name),
            Cell::new(&s.operation_name),
            Cell::new(&duration),
            Cell::new(&kind_str),
            Cell::new(&status_text),
        ]);
    }

    for line in table.to_string().lines() {
        println!("  {}", line);
    }
}

// -----------------------------------------------------------------------
// Log output
// -----------------------------------------------------------------------

pub fn print_logs(logs: &[StoredLog], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(logs).unwrap_or_default());
        }
        OutputFormat::Jsonl => {
            for l in logs {
                println!("{}", serde_json::to_string(l).unwrap_or_default());
            }
        }
        OutputFormat::Table => print_logs_table(logs),
    }
}

fn print_logs_table(logs: &[StoredLog]) {
    if logs.is_empty() {
        println!("  No logs found.");
        return;
    }

    let use_color = std::io::stdout().is_terminal();
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Time").set_alignment(CellAlignment::Left),
        Cell::new("Service").set_alignment(CellAlignment::Left),
        Cell::new("Level").set_alignment(CellAlignment::Center),
        Cell::new("Body").set_alignment(CellAlignment::Left),
    ]);

    for l in logs {
        let time = l.timestamp.format("%H:%M:%S%.3f").to_string();

        let severity_text = if use_color {
            match l.severity {
                LogSeverity::Error | LogSeverity::Fatal => {
                    format!("{}", format!("{:?}", l.severity).red())
                }
                LogSeverity::Warn => format!("{}", format!("{:?}", l.severity).yellow()),
                LogSeverity::Debug | LogSeverity::Trace => {
                    format!("{}", format!("{:?}", l.severity).dimmed())
                }
                _ => format!("{:?}", l.severity),
            }
        } else {
            format!("{:?}", l.severity)
        };

        // Truncate body for table display
        let body = if l.body.len() > 120 {
            format!("{}...", &l.body[..117])
        } else {
            l.body.clone()
        };

        table.add_row(vec![
            Cell::new(&time),
            Cell::new(&l.service_name),
            Cell::new(&severity_text),
            Cell::new(&body),
        ]);
    }

    for line in table.to_string().lines() {
        println!("  {}", line);
    }
}

// -----------------------------------------------------------------------
// Metric output
// -----------------------------------------------------------------------

pub fn print_metrics(metrics: &[StoredMetric], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(metrics).unwrap_or_default()
            );
        }
        OutputFormat::Jsonl => {
            for m in metrics {
                println!("{}", serde_json::to_string(m).unwrap_or_default());
            }
        }
        OutputFormat::Table => print_metrics_table(metrics),
    }
}

fn print_metrics_table(metrics: &[StoredMetric]) {
    if metrics.is_empty() {
        println!("  No metrics found.");
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Time").set_alignment(CellAlignment::Left),
        Cell::new("Service").set_alignment(CellAlignment::Left),
        Cell::new("Metric").set_alignment(CellAlignment::Left),
        Cell::new("Type").set_alignment(CellAlignment::Center),
        Cell::new("Value").set_alignment(CellAlignment::Right),
        Cell::new("Unit").set_alignment(CellAlignment::Center),
    ]);

    for m in metrics {
        let time = m.timestamp.format("%H:%M:%S%.3f").to_string();
        let type_str = format!("{:?}", m.metric_type);
        let value_str = format_metric_value(m.value);
        let unit = m.unit.as_deref().unwrap_or("-");

        table.add_row(vec![
            Cell::new(&time),
            Cell::new(&m.service_name),
            Cell::new(&m.metric_name),
            Cell::new(&type_str),
            Cell::new(&value_str),
            Cell::new(unit),
        ]);
    }

    for line in table.to_string().lines() {
        println!("  {}", line);
    }
}

// -----------------------------------------------------------------------
// Status output
// -----------------------------------------------------------------------

pub fn print_status(status: &SystemStatus, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(status).unwrap_or_default()
            );
        }
        OutputFormat::Jsonl | OutputFormat::Table => print_status_table(status),
    }
}

fn print_status_table(status: &SystemStatus) {
    let use_color = std::io::stdout().is_terminal();

    println!();
    if use_color {
        println!("  {} OTel Collector Status", "\u{25cf}".green());
    } else {
        println!("  OTel Collector Status");
    }
    println!();
    println!("  Traces:  {}", status.trace_count);
    println!("  Spans:   {}", status.span_count);
    println!("  Logs:    {}", status.log_count);
    println!("  Metrics: {}", status.metric_count);
    println!();

    if status.services.is_empty() {
        println!("  No services reporting telemetry.");
    } else {
        println!("  Services: {}", status.services.join(", "));
    }
    println!();
}

// -----------------------------------------------------------------------
// Related telemetry output
// -----------------------------------------------------------------------

pub fn print_related(related: &RelatedTelemetry, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(related).unwrap_or_default()
            );
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(related).unwrap_or_default());
        }
        OutputFormat::Table => {
            if !related.logs.is_empty() {
                println!("  Related Logs ({}):", related.logs.len());
                print_logs(&related.logs, OutputFormat::Table);
            }
            if !related.metrics.is_empty() {
                println!("  Related Metrics ({}):", related.metrics.len());
                print_metrics(&related.metrics, OutputFormat::Table);
            }
            if related.logs.is_empty() && related.metrics.is_empty() {
                println!("  No related telemetry found.");
            }
        }
    }
}

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn format_duration_ms(ms: u64) -> String {
    if ms < 1 {
        "<1ms".to_string()
    } else if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1_000;
        format!("{}m{}s", mins, secs)
    }
}

fn format_metric_value(v: f64) -> String {
    if v == v.floor() && v.abs() < 1_000_000.0 {
        format!("{}", v as i64)
    } else {
        format!("{:.3}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_sub_ms() {
        assert_eq!(format_duration_ms(0), "<1ms");
    }

    #[test]
    fn format_duration_milliseconds() {
        assert_eq!(format_duration_ms(42), "42ms");
        assert_eq!(format_duration_ms(999), "999ms");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(30_000), "30.0s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration_ms(90_000), "1m30s");
    }

    #[test]
    fn format_metric_integer() {
        assert_eq!(format_metric_value(42.0), "42");
    }

    #[test]
    fn format_metric_decimal() {
        assert_eq!(format_metric_value(3.14159), "3.142");
    }

    #[test]
    fn output_format_from_str() {
        assert_eq!(OutputFormat::from_str_opt(None), OutputFormat::Table);
        assert_eq!(OutputFormat::from_str_opt(Some("json")), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_str_opt(Some("jsonl")),
            OutputFormat::Jsonl
        );
        assert_eq!(
            OutputFormat::from_str_opt(Some("table")),
            OutputFormat::Table
        );
    }
}
