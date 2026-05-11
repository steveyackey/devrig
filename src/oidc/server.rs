use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::model::{OidcClientConfig, OidcConfig, OidcUserConfig};

use yauth::backends::memory::InMemoryBackend;
use yauth::config::{
    BearerConfig, EmailPasswordConfig, OAuth2ServerConfig, OidcConfig as YauthOidcConfig,
    SigningAlgorithm, YAuthConfig,
};
use yauth::state::YAuthState;
use yauth::YAuthBuilder;
use yauth_entity::{NewOauth2Client, NewPassword, NewUser};

/// Handle returned from [`allocate_oidc_runtime`].
///
/// Holds the resolved port + issuer so the orchestrator can publish
/// `oidc.port` / `oidc.issuer` template vars to other services. Pass to
/// [`launch_oidc_server`] once template resolution has finalised the
/// `redirect_uris` for each client.
#[derive(Debug, Clone)]
pub struct OidcRuntime {
    pub port: u16,
    pub issuer: String,
}

/// Reserve the OIDC port + compute the issuer URL, but do NOT bind the server
/// yet. Called during Phase 0.6 of orchestrator startup so other services can
/// reference `oidc.port` / `oidc.issuer` during template resolution.
pub fn allocate_oidc_runtime(port: u16, config: &OidcConfig) -> OidcRuntime {
    let issuer = config
        .issuer
        .clone()
        .unwrap_or_else(|| format!("http://localhost:{port}"));
    OidcRuntime { port, issuer }
}

/// Bind the in-process OIDC provider on `runtime.port`, seed users + clients
/// from `config`, then spawn the axum server on the orchestrator's tracker.
///
/// Called AFTER orchestrator template resolution so `config.clients[*].redirect_uris`
/// already have template variables (e.g. `{{ services.web.port }}`) expanded.
pub async fn launch_oidc_server(
    runtime: OidcRuntime,
    config: OidcConfig,
    cancel: CancellationToken,
    tracker: &tokio_util::task::TaskTracker,
) -> Result<()> {
    let port = runtime.port;
    let issuer = runtime.issuer.clone();

    let base_url = issuer.clone();
    let realm = config.realm.clone();
    let audience = config.audience.clone();

    // Build the yauth instance. Keep the surface narrow: only the plugins
    // needed for an OIDC code-flow + bearer issuance, all backed by the
    // in-memory store so we never touch disk.
    let backend = InMemoryBackend::new();
    let consent_ui_url = format!("{base_url}/login");

    let auth = YAuthBuilder::new(
        backend,
        YAuthConfig {
            base_url: base_url.clone(),
            session_cookie_name: "devrig_oidc_session".to_string(),
            session_ttl: Duration::from_secs(8 * 3600),
            secure_cookies: false,
            trusted_origins: vec![base_url.clone()],
            allow_signups: false,
            auto_admin_first_user: false,
            ..Default::default()
        },
    )
    .with_email_password(EmailPasswordConfig {
        min_password_length: 1,
        require_email_verification: false,
        hibp_check: false,
        ..Default::default()
    })
    .with_bearer(BearerConfig {
        jwt_secret: "devrig-oidc-hs256-not-used".to_string(),
        access_token_ttl: Duration::from_secs(15 * 60),
        refresh_token_ttl: Duration::from_secs(30 * 24 * 3600),
        audience: audience.clone(),
        signing_algorithm: SigningAlgorithm::Rs256,
        signing_key_pem: Some(generate_rsa_signing_key()?),
        kid: None,
    })
    .with_oauth2_server(OAuth2ServerConfig {
        issuer: issuer.clone(),
        authorization_code_ttl: Duration::from_secs(60),
        scopes_supported: vec!["openid".into(), "profile".into(), "email".into()],
        allow_dynamic_registration: false,
        consent_ui_url: Some(consent_ui_url.clone()),
        ..Default::default()
    })
    .with_oidc(YauthOidcConfig {
        issuer: issuer.clone(),
        ..Default::default()
    })
    .build()
    .await
    .map_err(|e| anyhow::anyhow!("building yauth OIDC provider: {e}"))?;

    seed_users(auth.state(), &config.users).await?;
    seed_clients(auth.state(), &config.clients).await?;

    let auth_state = auth.state().clone();
    let realm_for_ui = realm.clone();
    let issuer_for_ui = issuer.clone();

    let app = Router::new()
        // Built-in login + consent UI mounted at /login. yauth's OAuth2 server
        // forwards browsers here via `consent_ui_url`.
        .route(
            "/login",
            get(login_page).with_state(LoginPageState {
                realm: Arc::new(realm_for_ui),
                issuer: Arc::new(issuer_for_ui),
            }),
        )
        // yauth routes (login, register, /oauth/*, /userinfo, /.well-known/*)
        .merge(auth.router().with_state(auth_state))
        .layer(CorsLayer::permissive());

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding OIDC provider on {addr}"))?;

    info!(port, %issuer, realm = %config.realm, "OIDC provider listening");

    let shutdown = cancel.clone();
    tracker.spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
        {
            warn!(error = %e, "OIDC provider server exited with error");
        }
    });

    Ok(())
}

async fn seed_users(state: &YAuthState, users: &[OidcUserConfig]) -> Result<()> {
    for user in users {
        if state
            .repos
            .users
            .find_by_email(&user.email)
            .await
            .map_err(|e| anyhow::anyhow!("looking up seeded user {}: {e}", user.email))?
            .is_some()
        {
            continue;
        }

        let now = Utc::now().naive_utc();
        let user_id = Uuid::now_v7();
        let role = user.role.clone().unwrap_or_else(|| "user".to_string());

        state
            .repos
            .users
            .create(NewUser {
                id: user_id,
                email: user.email.clone(),
                display_name: user.name.clone(),
                email_verified: true,
                role,
                banned: false,
                banned_reason: None,
                banned_until: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .map_err(|e| anyhow::anyhow!("creating seeded user {}: {e}", user.email))?;

        let password_hash = yauth::auth::password::hash_password(&user.password)
            .await
            .map_err(|e| anyhow::anyhow!("hashing seeded user password: {e}"))?;

        state
            .repos
            .passwords
            .upsert(NewPassword {
                user_id,
                password_hash,
            })
            .await
            .map_err(|e| anyhow::anyhow!("storing seeded password: {e}"))?;

        debug!(email = %user.email, "seeded OIDC user");
    }
    Ok(())
}

async fn seed_clients(
    state: &YAuthState,
    clients: &std::collections::BTreeMap<String, OidcClientConfig>,
) -> Result<()> {
    for (client_id, client) in clients {
        if state
            .repos
            .oauth2_clients
            .find_by_client_id(client_id)
            .await
            .map_err(|e| anyhow::anyhow!("looking up seeded client {client_id}: {e}"))?
            .is_some()
        {
            continue;
        }

        let client_secret_hash = if client.public {
            None
        } else {
            client
                .client_secret
                .as_ref()
                .map(|s| yauth::auth::crypto::hash_token(s))
        };

        let grant_types = client.grant_types.clone().unwrap_or_else(|| {
            vec!["authorization_code".to_string(), "refresh_token".to_string()]
        });

        let scopes = client
            .scopes
            .as_ref()
            .map(|s| serde_json::json!(s.clone()))
            .or_else(|| Some(serde_json::json!(["openid", "profile", "email"])));

        let token_auth_method = if client.public {
            "none"
        } else {
            "client_secret_post"
        };

        state
            .repos
            .oauth2_clients
            .create(NewOauth2Client {
                id: Uuid::now_v7(),
                client_id: client_id.clone(),
                client_secret_hash,
                redirect_uris: serde_json::json!(client.redirect_uris),
                client_name: client.client_name.clone().or_else(|| Some(client_id.clone())),
                grant_types: serde_json::json!(grant_types),
                scopes,
                is_public: client.public,
                created_at: Utc::now().naive_utc(),
                token_endpoint_auth_method: Some(token_auth_method.to_string()),
                public_key_pem: None,
                jwks_uri: None,
            })
            .await
            .map_err(|e| anyhow::anyhow!("creating seeded client {client_id}: {e}"))?;

        debug!(client_id, public = client.public, "seeded OIDC client");
    }
    Ok(())
}

// ----- Login + consent page -----------------------------------------------

#[derive(Clone)]
struct LoginPageState {
    realm: Arc<String>,
    issuer: Arc<String>,
}

#[derive(Deserialize)]
struct ConsentParams {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

async fn login_page(
    State(state): State<LoginPageState>,
    Query(params): Query<ConsentParams>,
) -> Response {
    // Reject malformed params before rendering — the HTML form trusts these.
    if params.response_type != "code" {
        return Redirect::to(&format!(
            "{}?error=unsupported_response_type",
            params.redirect_uri
        ))
        .into_response();
    }

    Html(super::ui::render_login_page(
        &state.realm,
        &state.issuer,
        &params.client_id,
        &params.redirect_uri,
        &params.response_type,
        &params.code_challenge,
        &params.code_challenge_method,
        params.scope.as_deref(),
        params.state.as_deref(),
    ))
    .into_response()
}

// ----- Helpers ------------------------------------------------------------

/// Generate a fresh RS256 PKCS#8 PEM signing key. yauth re-uses this for all
/// tokens issued during the orchestrator's lifetime, so it does not need to
/// persist across restarts — clients re-fetch JWKS on key rotation anyway.
fn generate_rsa_signing_key() -> Result<String> {
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::RsaPrivateKey;

    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048).context("generating OIDC RSA signing key")?;
    let pem = key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .context("encoding OIDC signing key to PEM")?;
    Ok(pem.to_string())
}
