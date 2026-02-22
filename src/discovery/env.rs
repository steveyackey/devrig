use std::collections::{BTreeMap, HashMap};

use crate::config::model::DevrigConfig;
use crate::discovery::url::generate_url;

/// Build the full environment variable map for a given service.
///
/// The layering order (later overrides earlier):
/// 1. Global env from config.env
/// 2. Auto-generated DEVRIG_* vars for all infra
/// 3. Auto-generated DEVRIG_* vars for all other services
/// 4. PORT and HOST for the service itself
/// 5. Service-specific env (explicit overrides)
pub fn build_service_env(
    service_name: &str,
    config: &DevrigConfig,
    resolved_ports: &HashMap<String, u16>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();

    // 1. Start with global env
    for (k, v) in &config.env {
        env.insert(k.clone(), v.clone());
    }

    // 2. Add DEVRIG_* vars for all infra
    for (infra_name, infra_config) in &config.infra {
        let upper = infra_name.to_uppercase();
        let port_key = format!("infra:{}", infra_name);

        env.insert(format!("DEVRIG_{}_HOST", upper), "localhost".to_string());

        if let Some(&port) = resolved_ports.get(&port_key) {
            env.insert(format!("DEVRIG_{}_PORT", upper), port.to_string());
            let url = generate_url(infra_name, infra_config, port);
            env.insert(format!("DEVRIG_{}_URL", upper), url);
        }

        // Named ports
        for port_name in infra_config.ports.keys() {
            let named_key = format!("infra:{}:{}", infra_name, port_name);
            if let Some(&port) = resolved_ports.get(&named_key) {
                let upper_port_name = port_name.to_uppercase();
                env.insert(
                    format!("DEVRIG_{}_PORT_{}", upper, upper_port_name),
                    port.to_string(),
                );
            }
        }
    }

    // 3. Add DEVRIG_* vars for all other services
    for svc_name in config.services.keys() {
        if svc_name == service_name {
            continue;
        }
        let upper = svc_name.to_uppercase();
        let svc_key = format!("service:{}", svc_name);

        env.insert(format!("DEVRIG_{}_HOST", upper), "localhost".to_string());

        if let Some(&port) = resolved_ports.get(&svc_key) {
            env.insert(format!("DEVRIG_{}_PORT", upper), port.to_string());
            env.insert(
                format!("DEVRIG_{}_URL", upper),
                format!("http://localhost:{}", port),
            );
        }
    }

    // 4. Inject PORT and HOST for the service itself
    let own_key = format!("service:{}", service_name);
    if let Some(&port) = resolved_ports.get(&own_key) {
        env.insert("PORT".to_string(), port.to_string());
    }
    env.insert("HOST".to_string(), "localhost".to_string());

    // 5. Apply service-specific env (overrides auto-generated)
    if let Some(svc_config) = config.services.get(service_name) {
        for (k, v) in &svc_config.env {
            env.insert(k.clone(), v.clone());
        }
    }

    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{DevrigConfig, InfraConfig, Port, ProjectConfig, ServiceConfig};

    fn minimal_config() -> DevrigConfig {
        DevrigConfig {
            project: ProjectConfig {
                name: "test".to_string(),
            },
            services: BTreeMap::new(),
            infra: BTreeMap::new(),
            compose: None,
            cluster: None,
            env: BTreeMap::new(),
            network: None,
        }
    }

    fn make_infra(image: &str, env: Vec<(&str, &str)>) -> InfraConfig {
        InfraConfig {
            image: image.to_string(),
            port: None,
            ports: BTreeMap::new(),
            env: env
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            volumes: Vec::new(),
            ready_check: None,
            init: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    fn make_service(command: &str, port: Option<u16>) -> ServiceConfig {
        ServiceConfig {
            path: None,
            command: command.to_string(),
            port: port.map(Port::Fixed),
            env: BTreeMap::new(),
            depends_on: Vec::new(),
        }
    }

    #[test]
    fn infra_vars_present() {
        let mut config = minimal_config();
        let mut pg = make_infra(
            "postgres:16-alpine",
            vec![("POSTGRES_USER", "devrig"), ("POSTGRES_PASSWORD", "secret")],
        );
        pg.port = Some(Port::Fixed(5432));
        config.infra.insert("postgres".into(), pg);
        config
            .services
            .insert("api".into(), make_service("cargo run", Some(3000)));

        let mut ports = HashMap::new();
        ports.insert("infra:postgres".into(), 5432u16);
        ports.insert("service:api".into(), 3000u16);

        let env = build_service_env("api", &config, &ports);
        assert_eq!(env["DEVRIG_POSTGRES_HOST"], "localhost");
        assert_eq!(env["DEVRIG_POSTGRES_PORT"], "5432");
        assert_eq!(
            env["DEVRIG_POSTGRES_URL"],
            "postgres://devrig:secret@localhost:5432"
        );
    }

    #[test]
    fn named_port_vars() {
        let mut config = minimal_config();
        let mut mailpit = make_infra("axllent/mailpit:latest", vec![]);
        mailpit.ports.insert("smtp".into(), Port::Fixed(1025));
        mailpit.ports.insert("ui".into(), Port::Fixed(8025));
        config.infra.insert("mailpit".into(), mailpit);
        config
            .services
            .insert("api".into(), make_service("cargo run", Some(3000)));

        let mut ports = HashMap::new();
        ports.insert("infra:mailpit".into(), 1025u16);
        ports.insert("infra:mailpit:smtp".into(), 1025u16);
        ports.insert("infra:mailpit:ui".into(), 8025u16);
        ports.insert("service:api".into(), 3000u16);

        let env = build_service_env("api", &config, &ports);
        assert_eq!(env["DEVRIG_MAILPIT_HOST"], "localhost");
        assert_eq!(env["DEVRIG_MAILPIT_PORT_SMTP"], "1025");
        assert_eq!(env["DEVRIG_MAILPIT_PORT_UI"], "8025");
    }

    #[test]
    fn service_own_port_host() {
        let mut config = minimal_config();
        config
            .services
            .insert("api".into(), make_service("cargo run", Some(3000)));

        let mut ports = HashMap::new();
        ports.insert("service:api".into(), 3000u16);

        let env = build_service_env("api", &config, &ports);
        assert_eq!(env["PORT"], "3000");
        assert_eq!(env["HOST"], "localhost");
    }

    #[test]
    fn service_env_overrides() {
        let mut config = minimal_config();
        config.env.insert("RUST_LOG".into(), "info".into());
        let mut svc = make_service("cargo run", Some(3000));
        svc.env.insert("RUST_LOG".into(), "debug".into());
        svc.env.insert("HOST".into(), "0.0.0.0".into());
        config.services.insert("api".into(), svc);

        let mut ports = HashMap::new();
        ports.insert("service:api".into(), 3000u16);

        let env = build_service_env("api", &config, &ports);
        // Service-specific env overrides global and auto-generated
        assert_eq!(env["RUST_LOG"], "debug");
        assert_eq!(env["HOST"], "0.0.0.0");
    }

    #[test]
    fn service_to_service_discovery() {
        let mut config = minimal_config();
        config
            .services
            .insert("api".into(), make_service("cargo run", Some(3000)));
        config
            .services
            .insert("web".into(), make_service("npm run dev", Some(4000)));

        let mut ports = HashMap::new();
        ports.insert("service:api".into(), 3000u16);
        ports.insert("service:web".into(), 4000u16);

        // From web's perspective, it should see api's vars
        let env = build_service_env("web", &config, &ports);
        assert_eq!(env["DEVRIG_API_HOST"], "localhost");
        assert_eq!(env["DEVRIG_API_PORT"], "3000");
        assert_eq!(env["DEVRIG_API_URL"], "http://localhost:3000");
        // web should NOT see its own DEVRIG_WEB_* vars
        assert!(!env.contains_key("DEVRIG_WEB_HOST"));

        // From api's perspective, it should see web's vars
        let env2 = build_service_env("api", &config, &ports);
        assert_eq!(env2["DEVRIG_WEB_HOST"], "localhost");
        assert_eq!(env2["DEVRIG_WEB_PORT"], "4000");
        assert_eq!(env2["DEVRIG_WEB_URL"], "http://localhost:4000");
        assert!(!env2.contains_key("DEVRIG_API_HOST"));
    }
}
