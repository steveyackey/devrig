//! HTML for the built-in login + consent page.
//!
//! Rendered by `GET /login`. The visible form takes email + password and
//! POSTs to yauth's `/login` via fetch to establish a session cookie. Once
//! the session is set, the page programmatically submits a *hidden* HTML
//! form to `/oauth/authorize` — letting the browser natively follow yauth's
//! 303 redirect back to the OIDC client's `redirect_uri?code=…&state=…`.
//!
//! We can't use `fetch` for the consent step: with `redirect: "manual"` the
//! response is opaque and `Response.url` reflects the request URL, not the
//! `Location` target, which sent the browser back to `/oauth/authorize`
//! with no query string. Form submission sidesteps that.

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
    // Escape the values that appear as `value="…"` attributes on the hidden
    // consent form. These are user-controllable (they come in via query
    // params), so escaping is required to keep the form's value boundary.
    let issuer_attr = html_escape(issuer);
    let client_id_attr = html_escape(client_id);
    let redirect_uri_attr = html_escape(redirect_uri);
    let response_type_attr = html_escape(response_type);
    let code_challenge_attr = html_escape(code_challenge);
    let code_challenge_method_attr = html_escape(code_challenge_method);
    let scope_attr = scope.map(html_escape).unwrap_or_default();
    let state_attr = state.map(html_escape).unwrap_or_default();
    let scope_input = if scope.is_some() {
        format!(r#"<input type="hidden" name="scope" value="{scope_attr}">"#)
    } else {
        String::new()
    };
    let state_input = if state.is_some() {
        format!(r#"<input type="hidden" name="state" value="{state_attr}">"#)
    } else {
        String::new()
    };

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
  /* The consent form is submitted programmatically; never shown. */
  #consent-form {{ display: none; }}
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

  <!--
    Hidden consent form. We programmatically submit this after /login succeeds,
    so the browser natively follows yauth's 303 redirect to the OIDC client's
    redirect_uri (?code=…&state=…). Using a real form is what makes the
    cross-origin redirect work — fetch with `redirect: "manual"` returns an
    opaque response whose `url` is the request URL, not the Location header,
    which previously sent the browser back to /oauth/authorize with no query.
  -->
  <form id="consent-form" method="POST" action="{issuer_attr}/oauth/authorize">
    <input type="hidden" name="client_id" value="{client_id_attr}">
    <input type="hidden" name="redirect_uri" value="{redirect_uri_attr}">
    <input type="hidden" name="response_type" value="{response_type_attr}">
    <input type="hidden" name="code_challenge" value="{code_challenge_attr}">
    <input type="hidden" name="code_challenge_method" value="{code_challenge_method_attr}">
    {scope_input}
    {state_input}
    <input type="hidden" name="approved" value="true">
  </form>

<script>
(() => {{
  const issuer = {issuer_js};
  const loginForm = document.getElementById("login-form");
  const consentForm = document.getElementById("consent-form");
  const errBox = document.getElementById("error");
  const submitBtn = document.getElementById("submit");

  loginForm.addEventListener("submit", async (ev) => {{
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

      // Session cookie is now set; trigger the hidden consent form to POST
      // and let the browser follow yauth's 303 back to the OIDC client.
      submitBtn.textContent = "Redirecting…";
      consentForm.submit();
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
        issuer_attr = issuer_attr,
        client_id_attr = client_id_attr,
        redirect_uri_attr = redirect_uri_attr,
        response_type_attr = response_type_attr,
        code_challenge_attr = code_challenge_attr,
        code_challenge_method_attr = code_challenge_method_attr,
        scope_input = scope_input,
        state_input = state_input,
        issuer_js = json_str(issuer),
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
