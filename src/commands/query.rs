use anyhow::{bail, Context, Result};
use reqwest::Client;

use crate::config::resolve::resolve_config;
use crate::orchestrator::state::ProjectState;
use crate::otel::query::{RelatedTelemetry, SystemStatus, TraceDetail, TraceSummary};
use crate::otel::types::{StoredLog, StoredMetric};
use crate::query::output::{self, OutputFormat};

use std::path::Path;

/// Resolve dashboard HTTP base URL from project state.
fn dashboard_url(config_path: Option<&Path>) -> Result<String> {
    let config_path = match config_path {
        Some(p) => p.to_path_buf(),
        None => resolve_config(None)?,
    };
    let project_dir = config_path.parent().unwrap_or(Path::new("."));
    let state_dir = ProjectState::state_dir_for(project_dir);

    let state = ProjectState::load(&state_dir)
        .ok_or_else(|| anyhow::anyhow!("no running project found -- is devrig start running?"))?;

    let dash = state
        .dashboard
        .ok_or_else(|| anyhow::anyhow!("dashboard is not enabled in this project"))?;

    Ok(format!("http://localhost:{}", dash.dashboard_port))
}

pub async fn run_traces(
    config_path: Option<&Path>,
    service: Option<String>,
    status: Option<String>,
    min_duration: Option<u64>,
    limit: usize,
    output: Option<String>,
) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let mut url = format!("{}/api/traces?limit={}", base_url, limit);
    if let Some(ref svc) = service {
        url.push_str(&format!("&service={}", svc));
    }
    if let Some(ref s) = status {
        url.push_str(&format!("&status={}", s));
    }
    if let Some(d) = min_duration {
        url.push_str(&format!("&min_duration_ms={}", d));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let traces: Vec<TraceSummary> = resp.json().await.context("parsing trace response")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_traces(&traces, format);
    Ok(())
}

pub async fn run_trace_detail(
    config_path: Option<&Path>,
    trace_id: String,
    output: Option<String>,
) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let url = format!("{}/api/traces/{}", base_url, trace_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("trace '{}' not found", trace_id);
    }
    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let detail: TraceDetail = resp.json().await.context("parsing trace detail")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_spans(&detail.spans, format);
    Ok(())
}

pub async fn run_logs(
    config_path: Option<&Path>,
    service: Option<String>,
    severity: Option<String>,
    search: Option<String>,
    trace_id: Option<String>,
    limit: usize,
    output: Option<String>,
) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let mut url = format!("{}/api/logs?limit={}", base_url, limit);
    if let Some(ref svc) = service {
        url.push_str(&format!("&service={}", svc));
    }
    if let Some(ref sev) = severity {
        url.push_str(&format!("&severity={}", sev));
    }
    if let Some(ref s) = search {
        url.push_str(&format!("&search={}", s));
    }
    if let Some(ref tid) = trace_id {
        url.push_str(&format!("&trace_id={}", tid));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let logs: Vec<StoredLog> = resp.json().await.context("parsing log response")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_logs(&logs, format);
    Ok(())
}

pub async fn run_metrics(
    config_path: Option<&Path>,
    name: Option<String>,
    service: Option<String>,
    limit: usize,
    output: Option<String>,
) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let mut url = format!("{}/api/metrics?limit={}", base_url, limit);
    if let Some(ref n) = name {
        url.push_str(&format!("&name={}", n));
    }
    if let Some(ref svc) = service {
        url.push_str(&format!("&service={}", svc));
    }

    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let metrics: Vec<StoredMetric> = resp.json().await.context("parsing metric response")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_metrics(&metrics, format);
    Ok(())
}

pub async fn run_status(config_path: Option<&Path>, output: Option<String>) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let url = format!("{}/api/status", base_url);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let status: SystemStatus = resp.json().await.context("parsing status response")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_status(&status, format);
    Ok(())
}

pub async fn run_related(
    config_path: Option<&Path>,
    trace_id: String,
    output: Option<String>,
) -> Result<()> {
    let base_url = dashboard_url(config_path)?;
    let client = Client::new();

    let url = format!("{}/api/traces/{}/related", base_url, trace_id);
    let resp = client
        .get(&url)
        .send()
        .await
        .context("connecting to dashboard API")?;

    if !resp.status().is_success() {
        bail!("dashboard API returned {}", resp.status());
    }

    let related: RelatedTelemetry = resp.json().await.context("parsing related response")?;
    let format = OutputFormat::from_str_opt(output.as_deref());
    output::print_related(&related, format);
    Ok(())
}
