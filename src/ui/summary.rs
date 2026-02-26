use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{Cell, CellAlignment, Color, ContentArrangement, Table};
use is_terminal::IsTerminal;
use owo_colors::OwoColorize;

use crate::identity::ProjectIdentity;
use std::collections::BTreeMap;

pub struct StartupBannerInfo {
    pub services: Vec<String>,
    pub docker: Vec<String>,
    pub compose: Option<String>,
    pub cluster_addons: Vec<String>,
    pub dashboard_enabled: bool,
}

pub fn print_startup_banner(identity: &ProjectIdentity, info: &StartupBannerInfo) {
    let use_color = std::io::stdout().is_terminal();

    println!();
    if use_color {
        println!(
            "  {} {} {}",
            "Starting".bold(),
            "devrig".bold(),
            identity.name.cyan(),
        );
    } else {
        println!("  Starting devrig {}...", identity.name);
    }
    println!();

    if !info.services.is_empty() {
        println!("  Services:    {}", info.services.join(", "));
    }
    if !info.docker.is_empty() {
        println!("  Docker:      {}", info.docker.join(", "));
    }
    if let Some(compose) = &info.compose {
        println!("  Compose:     {}", compose);
    }
    if !info.cluster_addons.is_empty() {
        println!(
            "  Cluster:     k3s ({} addon{}: {})",
            info.cluster_addons.len(),
            if info.cluster_addons.len() == 1 { "" } else { "s" },
            info.cluster_addons.join(", "),
        );
    }
    if info.dashboard_enabled {
        println!("  Dashboard:   enabled");
    }
    println!();
}

pub struct RunningService {
    pub port: Option<u16>,
    pub port_auto: bool,
    pub status: String,
}

/// Print dashboard and OTLP endpoint info when dashboard is enabled.
pub fn print_dashboard_info(dash_port: u16, grpc_port: u16, http_port: u16) {
    let use_color = std::io::stdout().is_terminal();

    println!();
    if use_color {
        println!("  {}", "Dashboard".bold());
    } else {
        println!("  Dashboard");
    }
    println!("    URL:       http://localhost:{}", dash_port);
    println!("    OTLP gRPC: localhost:{}", grpc_port);
    println!("    OTLP HTTP: localhost:{}", http_port);
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

        let status_text = format!("\u{25cf} {}", svc.status);
        let status_color = if use_color {
            match svc.status.as_str() {
                "running" | "ready" => Some(Color::Green),
                "starting" => Some(Color::Yellow),
                "failed" => Some(Color::Red),
                _ => None,
            }
        } else {
            None
        };

        let mut status_cell = Cell::new(&status_text);
        if let Some(color) = status_color {
            status_cell = status_cell.fg(color);
        }

        table.add_row(vec![Cell::new(name), Cell::new(&url), status_cell]);
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
