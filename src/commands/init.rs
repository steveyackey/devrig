use anyhow::Result;
use std::path::Path;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("devrig.toml");

    if config_path.exists() {
        anyhow::bail!("devrig.toml already exists in {}", cwd.display());
    }

    // Detect project type
    let project_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string());

    let (service_name, service_command) = detect_project_type(&cwd);

    let config = format!(
        r#"[project]
name = "{project_name}"
# env_file = ".env"            # Load shared secrets from a .env file

# -- Global env vars shared by all services --
# [env]
# RUST_LOG = "debug"
# NODE_ENV = "development"
# SECRET_KEY = "$MY_SECRET_KEY" # $VAR expands from .env or host environment

# -- Dashboard + OpenTelemetry --
# Built-in dashboard and OTel collector. Services automatically receive
# OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_SERVICE_NAME. Ports auto-resolve
# if already in use, so multiple devrig instances can coexist.
[dashboard]
# port = 4000                    # default; auto-resolves if in use
# OTel defaults: grpc_port=4317, http_port=4318, retention="1h" — customize with [dashboard.otel]

# -- Services --
[services.{service_name}]
command = "{service_command}"
# port = 3000
# path = "./"
# depends_on = ["postgres"]
#
# env_file = ".env.{service_name}"  # Per-service .env file
#
# [services.{service_name}.env]
# DATABASE_URL = "postgres://user:${{DB_PASS}}@localhost:{{{{ docker.postgres.port }}}}/mydb"
#
# [services.{service_name}.restart]
# policy = "on-failure"
# max_restarts = 10

# [services.worker]
# command = "cargo run --bin worker"
# depends_on = ["{service_name}"]

# -- Docker containers --
# devrig manages Docker containers with health checks, init scripts, and volumes.
#
# [docker.postgres]
# image = "postgres:16-alpine"
# port = 5432
# volumes = ["pgdata:/var/lib/postgresql/data"]
# ready_check = {{ type = "pg_isready" }}
# init = ["CREATE DATABASE {project_name};"]
#
# [docker.postgres.env]
# POSTGRES_USER = "devrig"
# POSTGRES_PASSWORD = "devrig"
#
# [docker.redis]
# image = "redis:7-alpine"
# port = 6379
# ready_check = {{ type = "cmd", command = "redis-cli ping", expect = "PONG" }}
#
# -- Private registry images --
# [docker.my-app]
# image = "ghcr.io/org/app:latest"
# registry_auth = {{ username = "$REGISTRY_USER", password = "$REGISTRY_TOKEN" }}

# -- Docker Compose integration --
# Delegate to an existing docker-compose.yml.
# Services are auto-discovered from the file; list specific ones to limit.
#
# [compose]
# file = "docker-compose.yml"
# services = ["redis", "postgres"]  # Optional — empty auto-discovers all

# -- Kubernetes cluster (k3d) --
# Create a local cluster with auto-build and deploy.
#
# [cluster]
# agents = 1
# ports = ["8080:80"]
#
# [cluster.image.job-runner]
# context = "./tools/job-runner"
# # dockerfile = "Dockerfile"   # optional, defaults to Dockerfile
# watch = true
#
# [cluster.deploy.api]
# context = "./services/api"
# manifests = ["k8s/deployment.yaml", "k8s/service.yaml"]
# watch = true
# depends_on = ["job-runner"]   # ensures image is built before deploy
#
# [cluster.addons.traefik]
# type = "helm"
# chart = "traefik/traefik"
# repo = "https://traefik.github.io/charts"
# namespace = "traefik"
#
# -- Local chart (no repo needed) --
# [cluster.addons.myapp]
# type = "helm"
# chart = "./charts/myapp"
# namespace = "myapp"
# values_files = ["charts/myapp/values-dev.yaml"]
#
# -- Private registry auth for cluster image pulls --
# [[cluster.registries]]
# url = "ghcr.io"
# username = "$REGISTRY_USER"
# password = "$REGISTRY_TOKEN"
"#
    );

    std::fs::write(&config_path, &config)?;
    println!("Created devrig.toml in {}", cwd.display());
    println!();
    println!("  Project: {}", project_name);
    println!("  Service: {} -> {}", service_name, service_command);
    println!();
    println!("Edit the file, then run `devrig start` to begin.");
    Ok(())
}

fn detect_project_type(dir: &Path) -> (&'static str, &'static str) {
    if dir.join("Cargo.toml").exists() {
        ("app", "cargo watch -x run")
    } else if dir.join("package.json").exists() {
        ("app", "npm run dev")
    } else if dir.join("go.mod").exists() {
        ("app", "go run .")
    } else if dir.join("requirements.txt").exists() || dir.join("pyproject.toml").exists() {
        ("app", "python main.py")
    } else {
        ("app", "echo 'Replace this with your command'")
    }
}
