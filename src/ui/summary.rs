use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;

use crate::config::model::DashboardConfig;
use crate::identity::ProjectIdentity;
use std::collections::BTreeMap;

pub struct RunningService {
    pub port: Option<u16>,
    pub port_auto: bool,
    pub status: String,
}

/// Print dashboard and OTLP endpoint info when dashboard is enabled.
pub fn print_dashboard_info(dashboard: &DashboardConfig) {
    let use_color = std::io::stdout().is_terminal();
    let otel = dashboard.otel.clone().unwrap_or_default();

    println!();
    if use_color {
        println!("  {}", "Dashboard".bold());
    } else {
        println!("  Dashboard");
    }
    println!("    URL:       http://localhost:{}", dashboard.port);
    println!("    OTLP gRPC: localhost:{}", otel.grpc_port);
    println!("    OTLP HTTP: localhost:{}", otel.http_port);
}

pub fn print_startup_summary(
    identity: &ProjectIdentity,
    services: &BTreeMap<String, RunningService>,
) {
    let use_color = std::io::stdout().is_terminal();

    println!();
    if use_color {
        println!(
            "  {} {} ({})",
            "devrig".bold(),
            identity.name.cyan(),
            identity.id.dimmed()
        );
    } else {
        println!("  devrig {} ({})", identity.name, identity.id);
    }
    println!();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Service").set_alignment(CellAlignment::Left),
        Cell::new("URL").set_alignment(CellAlignment::Left),
        Cell::new("Status").set_alignment(CellAlignment::Center),
    ]);

    for (name, svc) in services {
        let url = svc
            .port
            .map(|p| {
                let base = if name.starts_with("[docker]") || name.starts_with("[cluster]") {
                    format!("localhost:{}", p)
                } else {
                    format!("http://localhost:{}", p)
                };
                if svc.port_auto {
                    format!("{} (auto)", base)
                } else {
                    base
                }
            })
            .unwrap_or_else(|| "-".to_string());

        let status_text = if use_color {
            match svc.status.as_str() {
                "running" => format!("{} {}", "\u{25cf}".green(), "running".green()),
                "ready" => format!("{} {}", "\u{25cf}".green(), "ready".green()),
                "starting" => format!("{} {}", "\u{25cf}".yellow(), "starting".yellow()),
                "failed" => format!("{} {}", "\u{25cf}".red(), "failed".red()),
                other => format!("\u{25cf} {}", other),
            }
        } else {
            format!("\u{25cf} {}", svc.status)
        };

        table.add_row(vec![
            Cell::new(name),
            Cell::new(&url),
            Cell::new(&status_text),
        ]);
    }

    // Indent the table by 2 spaces
    for line in table.to_string().lines() {
        println!("  {}", line);
    }

    if let Some(port) = resolve_dashboard_display_port(services) {
        println!();
        if use_color {
            println!(
                "  Dashboard: {}",
                format!("http://localhost:{}", port).cyan()
            );
        } else {
            println!("  Dashboard: http://localhost:{}", port);
        }
    }

    if services.keys().any(|name| name.starts_with("[cluster]")) {
        println!();
        if use_color {
            println!("  Use: {} get pods", "devrig k".bold());
        } else {
            println!("  Use: devrig k get pods");
        }
    }

    println!();
    if use_color {
        println!("  Press {} to stop", "Ctrl+C".bold());
    } else {
        println!("  Press Ctrl+C to stop");
    }
    println!();
}

/// Resolve which port to display as the dashboard URL.
/// Prefers the Vite dev server (live reload) when available,
/// otherwise falls back to the embedded dashboard port.
pub fn resolve_dashboard_display_port(
    services: &BTreeMap<String, RunningService>,
) -> Option<u16> {
    services
        .get("[vite]")
        .or_else(|| services.get("[dashboard]"))
        .and_then(|svc| svc.port)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc(port: u16) -> RunningService {
        RunningService {
            port: Some(port),
            port_auto: false,
            status: "running".to_string(),
        }
    }

    #[test]
    fn dashboard_port_shown_when_no_vite() {
        let mut services = BTreeMap::new();
        services.insert("[dashboard]".to_string(), svc(4000));
        assert_eq!(resolve_dashboard_display_port(&services), Some(4000));
    }

    #[test]
    fn vite_port_preferred_over_dashboard() {
        let mut services = BTreeMap::new();
        services.insert("[dashboard]".to_string(), svc(4000));
        services.insert("[vite]".to_string(), svc(5173));
        assert_eq!(resolve_dashboard_display_port(&services), Some(5173));
    }

    #[test]
    fn no_dashboard_or_vite_returns_none() {
        let mut services = BTreeMap::new();
        services.insert("api".to_string(), svc(3000));
        assert_eq!(resolve_dashboard_display_port(&services), None);
    }

    #[test]
    fn auto_resolved_dashboard_port_shown() {
        let mut services = BTreeMap::new();
        // Port auto-resolved to 4001 because 4000 was busy
        services.insert("[dashboard]".to_string(), svc(4001));
        assert_eq!(resolve_dashboard_display_port(&services), Some(4001));
    }
}
