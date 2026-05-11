//! Built-in OpenID Connect provider.
//!
//! Spins up a [`yauth`](https://github.com/yackey-labs/yauth)-backed in-memory
//! OIDC provider on a dedicated port. Users + clients are seeded from the
//! `[oidc]` block in `devrig.toml`, so projects can drop their Keycloak /
//! dex container entirely.
//!
//! The provider exposes:
//!   - `GET  /.well-known/openid-configuration`
//!   - `GET  /.well-known/jwks.json`
//!   - `GET  /oauth/authorize`         (redirects to `/login`)
//!   - `POST /oauth/authorize`         (consent submission)
//!   - `POST /oauth/token`             (token exchange)
//!   - `GET  /userinfo`
//!   - `POST /register`, `POST /login` (yauth email-password)
//!   - `GET  /login`                   (built-in HTML login + consent page)

pub mod server;
pub mod ui;

pub use server::{allocate_oidc_runtime, launch_oidc_server, OidcRuntime};
