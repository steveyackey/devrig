#![cfg(feature = "integration")]

use std::path::PathBuf;
use std::time::Duration;

use devrig::config::model::OtelConfig;
use devrig::dashboard::server::start_dashboard_server;
use devrig::otel::OtelCollector;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct ConfigResponse {
    content: String,
    hash: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConfigErrorResponse {
    error: String,
}

async fn start_config_stack(
    grpc_port: u16,
    http_port: u16,
    dashboard_port: u16,
    config_path: PathBuf,
) -> CancellationToken {
    let cancel = CancellationToken::new();

    let otel_config = OtelConfig {
        grpc_port,
        http_port,
        trace_buffer: 100,
        metric_buffer: 100,
        log_buffer: 100,
        retention: "1h".to_string(),
    };

    let collector = OtelCollector::new(&otel_config);
    collector.start(cancel.clone()).await.unwrap();

    let store = collector.store();
    let events_tx = collector.events_tx();

    let dash_cancel = cancel.clone();
    tokio::spawn(async move {
        let _ = start_dashboard_server(
            dashboard_port,
            store,
            events_tx,
            dash_cancel,
            Some(config_path),
        )
        .await;
    });

    // Give servers a moment to bind their ports.
    tokio::time::sleep(Duration::from_millis(500)).await;

    cancel
}

// ---------------------------------------------------------------------------
// Test 1: GET /api/config returns file content and a hash.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_get_returns_content_and_hash() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("devrig.toml");
    let content = "[project]\nname = \"test\"\n";
    std::fs::write(&config_file, content).unwrap();

    let cancel = start_config_stack(17117, 17118, 17100, config_file.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get("http://127.0.0.1:17100/api/config")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let config: ConfigResponse = resp.json().await.unwrap();
    assert_eq!(config.content, content);
    assert!(!config.hash.is_empty(), "hash should be non-empty");

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 2: PUT /api/config updates content and returns a new hash.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_put_updates_content() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("devrig.toml");
    let original = "[project]\nname = \"original\"\n";
    std::fs::write(&config_file, original).unwrap();

    let cancel = start_config_stack(17217, 17218, 17200, config_file.clone()).await;

    let client = reqwest::Client::new();

    // GET to obtain the current hash.
    let get_resp: ConfigResponse = client
        .get("http://127.0.0.1:17200/api/config")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // PUT with new content and correct hash.
    let new_content = "[project]\nname = \"updated\"\n";
    let put_resp = client
        .put("http://127.0.0.1:17200/api/config")
        .json(&serde_json::json!({
            "content": new_content,
            "hash": get_resp.hash,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_resp.status(), 200);

    let updated: ConfigResponse = put_resp.json().await.unwrap();
    assert_eq!(updated.content, new_content);
    assert_ne!(
        updated.hash, get_resp.hash,
        "hash should change after update"
    );

    // Verify the file was actually written to disk.
    let on_disk = std::fs::read_to_string(&config_file).unwrap();
    assert_eq!(on_disk, new_content);

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 3: PUT /api/config with wrong hash returns 409 Conflict.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_put_with_wrong_hash_returns_conflict() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("devrig.toml");
    std::fs::write(&config_file, "[project]\nname = \"test\"\n").unwrap();

    let cancel = start_config_stack(17317, 17318, 17300, config_file.clone()).await;

    let client = reqwest::Client::new();
    let put_resp = client
        .put("http://127.0.0.1:17300/api/config")
        .json(&serde_json::json!({
            "content": "[project]\nname = \"new\"\n",
            "hash": "wrong-hash",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        put_resp.status(),
        409,
        "wrong hash should return 409 Conflict"
    );

    cancel.cancel();
}

// ---------------------------------------------------------------------------
// Test 4: PUT /api/config with invalid TOML returns 400 Bad Request.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn config_put_with_invalid_toml_returns_bad_request() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("devrig.toml");
    let original = "[project]\nname = \"test\"\n";
    std::fs::write(&config_file, original).unwrap();

    let cancel = start_config_stack(17417, 17418, 17400, config_file.clone()).await;

    let client = reqwest::Client::new();

    // GET to obtain the current hash.
    let get_resp: ConfigResponse = client
        .get("http://127.0.0.1:17400/api/config")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // PUT with invalid TOML content.
    let put_resp = client
        .put("http://127.0.0.1:17400/api/config")
        .json(&serde_json::json!({
            "content": "this is not [valid toml =",
            "hash": get_resp.hash,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(put_resp.status(), 400, "invalid TOML should return 400");

    cancel.cancel();
}
