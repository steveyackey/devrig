use crate::identity::ProjectIdentity;
use std::collections::BTreeMap;

pub struct RunningService {
    pub port: Option<u16>,
    pub port_auto: bool,
    pub status: String,
}

pub fn print_startup_summary(
    identity: &ProjectIdentity,
    services: &BTreeMap<String, RunningService>,
) {
    println!();
    println!("  devrig \u{26a1} {} ({})", identity.name, identity.id);
    println!();

    let max_name_len = services.keys().map(|n| n.len()).max().unwrap_or(16).max(16);

    for (name, svc) in services {
        let url = svc
            .port
            .map(|p| {
                if name.starts_with("[infra]") {
                    format!("localhost:{}", p)
                } else {
                    format!("http://localhost:{}", p)
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let auto_tag = if svc.port_auto { " (auto)" } else { "" };
        println!(
            "    {:<width$} {:<30} \u{25cf} {}",
            name,
            format!("{}{}", url, auto_tag),
            svc.status,
            width = max_name_len,
        );
    }

    println!();
    println!("  Press Ctrl+C to stop");
    println!();
}
