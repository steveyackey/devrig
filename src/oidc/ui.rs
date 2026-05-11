//! HTML for the built-in login + consent page.
//!
//! Rendered by `GET /login`. The user enters email + password, the page
//! `POST`s to yauth's `/login` to establish a session, then `POST`s to
//! `/oauth/authorize` with the user's consent decision — yauth issues the
//! authorization code and 302s back to the OIDC client.

/// Build the HTML body for the login + consent page. Caller passes the OAuth2
/// parameters extracted from the incoming `/oauth/authorize` redirect; the
/// page round-trips them through yauth's consent endpoint after sign-in.
#[allow(clippy::too_many_arguments)]
pub fn render_login_page(
    realm: &str,
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    response_type: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    scope: Option<&str>,
    state: Option<&str>,
) -> String {
    let realm_esc = html_escape(realm);
    let client_esc = html_escape(client_id);
    let issuer_esc = html_escape(issuer);

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sign in — {realm_esc}</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
  :root {{ color-scheme: light dark; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    margin: 0; padding: 0;
    display: flex; align-items: center; justify-content: center;
    min-height: 100vh;
    background: #0b1020;
    color: #e7ecf3;
  }}
  .card {{
    width: min(420px, 92vw);
    background: #131a2e;
    border: 1px solid #243056;
    border-radius: 12px;
    padding: 28px 28px 24px;
    box-shadow: 0 10px 40px rgba(0,0,0,0.35);
  }}
  h1 {{ font-size: 18px; margin: 0 0 4px; letter-spacing: 0.2px; }}
  .sub {{ font-size: 12px; color: #8a93a8; margin-bottom: 22px; }}
  label {{ display: block; font-size: 12px; color: #b5bdd1; margin: 14px 0 6px; }}
  input[type="email"], input[type="password"] {{
    width: 100%; box-sizing: border-box;
    padding: 10px 12px;
    background: #0b1020;
    border: 1px solid #2a3766;
    border-radius: 8px;
    color: #e7ecf3;
    font-size: 14px;
  }}
  input:focus {{ outline: none; border-color: #6c8cff; }}
  button {{
    width: 100%; margin-top: 18px;
    padding: 10px 12px;
    background: #6c8cff;
    color: #0b1020;
    font-weight: 600;
    border: none; border-radius: 8px;
    cursor: pointer; font-size: 14px;
  }}
  button:hover {{ background: #859fff; }}
  button[disabled] {{ opacity: 0.6; cursor: progress; }}
  .err {{
    margin-top: 14px; padding: 10px 12px;
    background: #3a1a1a; border: 1px solid #6c2a2a;
    border-radius: 8px; color: #ffb1b1; font-size: 13px;
    display: none;
  }}
  .meta {{ margin-top: 18px; font-size: 11px; color: #6a7290; }}
  code {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; color: #aab4d4; }}
</style>
</head>
<body>
  <form id="login-form" class="card" autocomplete="on">
    <h1>Sign in to {realm_esc}</h1>
    <div class="sub">Client <code>{client_esc}</code> is requesting access.</div>

    <label for="email">Email</label>
    <input id="email" name="email" type="email" required autofocus autocomplete="username">

    <label for="password">Password</label>
    <input id="password" name="password" type="password" required autocomplete="current-password">

    <button id="submit" type="submit">Sign in &amp; authorize</button>
    <div id="error" class="err" role="alert"></div>

    <div class="meta">
      Issuer: <code>{issuer_esc}</code>
    </div>
  </form>

<script>
(() => {{
  const issuer = {issuer_js};
  const params = {{
    client_id: {client_id_js},
    redirect_uri: {redirect_uri_js},
    response_type: {response_type_js},
    code_challenge: {code_challenge_js},
    code_challenge_method: {code_challenge_method_js},
    scope: {scope_js},
    state: {state_js}
  }};
  const form = document.getElementById("login-form");
  const errBox = document.getElementById("error");
  const submitBtn = document.getElementById("submit");

  form.addEventListener("submit", async (ev) => {{
    ev.preventDefault();
    errBox.style.display = "none";
    submitBtn.disabled = true;
    submitBtn.textContent = "Signing in…";

    const email = document.getElementById("email").value.trim();
    const password = document.getElementById("password").value;

    try {{
      const loginRes = await fetch(issuer + "/login", {{
        method: "POST",
        headers: {{ "Content-Type": "application/json", "Accept": "application/json" }},
        credentials: "include",
        body: JSON.stringify({{ email, password }})
      }});
      if (!loginRes.ok) {{
        let detail = "Invalid credentials";
        try {{ const j = await loginRes.json(); if (j && j.error) detail = j.error; }} catch (_e) {{}}
        throw new Error(detail);
      }}

      const consentBody = {{
        client_id: params.client_id,
        redirect_uri: params.redirect_uri,
        response_type: params.response_type,
        code_challenge: params.code_challenge,
        code_challenge_method: params.code_challenge_method,
        approved: true
      }};
      if (params.scope) consentBody.scope = params.scope;
      if (params.state) consentBody.state = params.state;

      const consentRes = await fetch(issuer + "/oauth/authorize", {{
        method: "POST",
        headers: {{ "Content-Type": "application/json" }},
        credentials: "include",
        redirect: "manual",
        body: JSON.stringify(consentBody)
      }});

      if (consentRes.type === "opaqueredirect" || consentRes.status === 0) {{
        const url = new URL(params.redirect_uri);
        const qs = new URLSearchParams({{}});
        if (params.state) qs.set("state", params.state);
        window.location.href = consentRes.url || params.redirect_uri;
        return;
      }}

      if (consentRes.ok || (consentRes.status >= 300 && consentRes.status < 400)) {{
        const loc = consentRes.headers.get("Location");
        if (loc) {{ window.location.href = loc; return; }}
      }}

      const text = await consentRes.text();
      throw new Error("Authorization failed: " + text.slice(0, 200));
    }} catch (e) {{
      errBox.textContent = e.message || String(e);
      errBox.style.display = "block";
      submitBtn.disabled = false;
      submitBtn.textContent = "Sign in & authorize";
    }}
  }});
}})();
</script>
</body>
</html>
"#,
        realm_esc = realm_esc,
        client_esc = client_esc,
        issuer_esc = issuer_esc,
        issuer_js = json_str(issuer),
        client_id_js = json_str(client_id),
        redirect_uri_js = json_str(redirect_uri),
        response_type_js = json_str(response_type),
        code_challenge_js = json_str(code_challenge),
        code_challenge_method_js = json_str(code_challenge_method),
        scope_js = match scope {
            Some(s) => json_str(s),
            None => "null".to_string(),
        },
        state_js = match state {
            Some(s) => json_str(s),
            None => "null".to_string(),
        },
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}
