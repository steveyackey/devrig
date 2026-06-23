// Package oidc runs an in-process yauth-backed OIDC provider for local dev.
// It is seeded from [oidc] config and binds on its own port; no Postgres is
// required — the backend is the yauth "memory" driver.
//
// yauth v0.39+ is API-first: its /oauth/authorize endpoint speaks JSON, not
// HTML. To keep a browser OIDC flow working (so SPAs and Playwright can drive a
// real login), this package mounts the yauth API under /api/auth and serves two
// small server-rendered pages at the root:
//
//   - GET /login     — email/password form (#email/#password/#submit) that
//     POSTs to yauth's JSON login, then returns to the flow.
//   - GET /authorize — the consent page the discovery doc's
//     authorization_endpoint points at (via mcpauth.Mount). It
//     drives yauth's JSON authorize+consent and redirects the
//     browser back to the client with the code, auto-approving
//     consent for this local dev provider.
//
// This restores the server-rendered login that the pre-Go (Rust) devrig served
// from src/oidc/ui.rs, which downstream apps' E2E tests rely on.
package oidc

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"net/url"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	yauth "github.com/yackey-labs/yauth"
	"github.com/yackey-labs/yauth/auth"
	"github.com/yackey-labs/yauth/domain"
	"github.com/yackey-labs/yauth/mcpauth"
	"github.com/yackey-labs/yauth/yauthcfg"
	"golang.org/x/sync/singleflight"

	"github.com/steveyackey/devrig/internal/config"
)

// authBasePath is where the yauth JSON API is mounted on the public mux.
const authBasePath = "/api/auth"

// Server is an in-process OIDC provider.
type Server struct {
	cfg    *config.OIDCConfig
	port   uint16
	logger *slog.Logger

	// tokenExchange collapses concurrent authorization_code exchanges that
	// present the same code into a single upstream call, and tokenCache replays
	// the successful result to later repeats within a short window (see
	// coalesceTokenExchange).
	tokenExchange singleflight.Group
	tokenCacheMu  sync.Mutex
	tokenCache    map[string]tokenCacheEntry
}

type tokenCacheEntry struct {
	resp      capturedResponse
	expiresAt time.Time
}

// tokenReplayWindow is how long a successful authorization_code exchange is
// replayable by the same code. Long enough to absorb an SPA's repeated callback
// renders (double/triple mount, HMR), short enough that it's not a general code
// cache. Dev-only provider.
const tokenReplayWindow = 60 * time.Second

// New creates an OIDC server from the given config and resolved port.
func New(cfg *config.OIDCConfig, port uint16, logger *slog.Logger) *Server {
	if logger == nil {
		logger = slog.Default()
	}
	return &Server{cfg: cfg, port: port, logger: logger}
}

// Start builds the yauth instance, seeds users and clients, then listens.
// It blocks until ctx is cancelled.
func (s *Server) Start(ctx context.Context) error {
	issuer := fmt.Sprintf("http://localhost:%d", s.port)
	if s.cfg.Issuer != nil {
		issuer = *s.cfg.Issuer
	}

	// Generate an ephemeral RS256 keypair so id_tokens are signed
	// asymmetrically and a JWKS is published — relying parties can't verify
	// HS256-signed id_tokens (no jwks_uri). Regenerated each start, matching the
	// previous (Rust) provider.
	privPEM, pubPEM, err := generateRSAKeyPEM()
	if err != nil {
		return fmt.Errorf("oidc: generate signing key: %w", err)
	}
	const privEnv, pubEnv, secretEnv = "DEVRIG_OIDC_SIGNING_KEY", "DEVRIG_OIDC_SIGNING_PUB", "DEVRIG_OIDC_JWT_SECRET"

	// The bearer plugin requires a JWT secret (used for HS256 session tokens);
	// OAuth2 access/id tokens are still RS256 via asym_jwt. Generate a random
	// per-start secret.
	secretBytes := make([]byte, 32)
	if _, err := rand.Read(secretBytes); err != nil {
		return fmt.Errorf("oidc: generate jwt secret: %w", err)
	}
	os.Setenv(secretEnv, hex.EncodeToString(secretBytes))

	hibpDisabled := false
	allowSignups := false

	yauthCfg := &yauthcfg.Config{
		Database: yauthcfg.DatabaseConfig{Driver: "memory"},
		Server: yauthcfg.ServerConfig{
			// BaseURL is the public origin; the router is mounted under
			// authBasePath on the mux via StripPrefix, so Prefix stays empty
			// (Router serves at root) while plugin BasePath carries the prefix
			// into the generated discovery URLs.
			BaseURL:      issuer,
			AllowSignups: &allowSignups,
		},
		Plugins: yauthcfg.PluginsConfig{
			EmailPassword: yauthcfg.EmailPasswordPluginConfig{
				Enabled:                  true,
				RequireEmailVerification: false,
				HIBPCheck:                &hibpDisabled,
			},
			// Validates incoming Bearer (JWT) access tokens — required for
			// /userinfo and any token-protected endpoint. Without it, valid
			// access tokens are rejected (401), which breaks OIDC clients that
			// load userinfo after the code exchange.
			Bearer: yauthcfg.BearerPluginConfig{
				Enabled:      true,
				Issuer:       issuer,
				JWTSecretEnv: secretEnv,
			},
			AsymJWT: yauthcfg.AsymJWTPluginConfig{
				Enabled:          true,
				KeyType:          "rs256",
				PrivateKeyPEMEnv: privEnv,
				PublicKeyPEMEnv:  pubEnv,
				KeyID:            "devrig-oidc",
			},
			OAuth2Server: yauthcfg.OAuth2ServerPluginConfig{
				Enabled:  true,
				Issuer:   issuer,
				BasePath: authBasePath,
			},
			OIDC: yauthcfg.OIDCPluginConfig{
				Enabled:  true,
				Issuer:   issuer,
				BasePath: authBasePath,
			},
		},
	}

	// AsymJWT reads the keys from these env vars at build time.
	os.Setenv(privEnv, string(privPEM))
	os.Setenv(pubEnv, string(pubPEM))

	// yauth is chatty at info/warn (per-request logs, the console-mailer
	// warning); route its internal logger at error level only so it doesn't
	// flood devrig's output. devrig's own oidc messages still use s.logger.
	quiet := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: slog.LevelError}))
	y, err := yauth.NewFromConfig(ctx, yauthCfg, yauth.WithConfigLogger(quiet))
	if err != nil {
		return fmt.Errorf("oidc: build yauth: %w", err)
	}

	repo := y.Repo()
	if err := s.seedUsers(ctx, repo); err != nil {
		return fmt.Errorf("oidc: seed users: %w", err)
	}
	if err := s.seedClients(ctx, repo); err != nil {
		return fmt.Errorf("oidc: seed clients: %w", err)
	}

	mux := http.NewServeMux()
	// Root discovery (RFC 8414 + OIDC) with authorization_endpoint rewritten to
	// /authorize, so a browser OIDC flow lands on the consent page below
	// instead of yauth's JSON authorize endpoint.
	mcpauth.Mount(mux, y, mcpauth.Config{
		AuthBasePath: authBasePath,
		PublicURL:    issuer,
		ConsentPath:  "/authorize",
	})
	// The yauth JSON API, with the token endpoint guarded so a relying party
	// that double-fires the code exchange (a common SPA/oidc-client-ts quirk —
	// the callback effect runs twice) still succeeds instead of getting a
	// single-use-code 400 on the second, concurrent request.
	mux.Handle(authBasePath+"/", s.coalesceTokenExchange(http.StripPrefix(authBasePath, y.Router())))
	// Server-rendered browser pages.
	mux.HandleFunc("/login", s.handleLoginPage)
	mux.HandleFunc("/authorize", s.handleConsentPage)

	ln, err := net.Listen("tcp", fmt.Sprintf(":%d", s.port))
	if err != nil {
		return fmt.Errorf("oidc: listen on port %d: %w", s.port, err)
	}

	srv := &http.Server{Handler: withCORS(withDiscoveryHostRewrite(mux))}
	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutCtx)
	}()

	s.logger.Debug("oidc provider started", "addr", ln.Addr().String(), "issuer", issuer)
	if serveErr := srv.Serve(ln); serveErr != nil && serveErr != http.ErrServerClosed {
		return fmt.Errorf("oidc: serve: %w", serveErr)
	}
	return nil
}

func (s *Server) seedUsers(ctx context.Context, repo interface {
	CreateUser(context.Context, domain.NewUser) (domain.User, error)
	UpsertPassword(context.Context, domain.NewPassword) error
}) error {
	for _, u := range s.cfg.Users {
		role := "user"
		if u.Role != nil {
			role = *u.Role
		}
		created, err := repo.CreateUser(ctx, domain.NewUser{
			ID:            uuid.New().String(),
			Email:         u.Email,
			DisplayName:   u.Name,
			EmailVerified: true,
			Role:          role,
			CreatedAt:     time.Now(),
			UpdatedAt:     time.Now(),
		})
		if err != nil {
			s.logger.Warn("oidc: seeding user (may already exist)", "email", u.Email, "err", err)
			continue
		}
		hash, err := auth.HashPassword(u.Password)
		if err != nil {
			return fmt.Errorf("hash password for %s: %w", u.Email, err)
		}
		if err := repo.UpsertPassword(ctx, domain.NewPassword{
			UserID:       created.ID,
			PasswordHash: hash,
		}); err != nil {
			return fmt.Errorf("set password for %s: %w", u.Email, err)
		}
	}
	return nil
}

func (s *Server) seedClients(ctx context.Context, repo interface {
	CreateOAuth2Client(context.Context, domain.NewOAuth2Client) error
}) error {
	for clientID, cc := range s.cfg.Clients {
		redirectURIsJSON, _ := json.Marshal(cc.RedirectURIs)

		grantTypes := cc.GrantTypes
		if len(grantTypes) == 0 {
			grantTypes = []string{"authorization_code", "refresh_token"}
		}
		grantTypesJSON, _ := json.Marshal(grantTypes)

		scopes := cc.Scopes
		if len(scopes) == 0 {
			scopes = []string{"openid", "profile", "email"}
		}
		scopesJSON, _ := json.Marshal(scopes)

		var secretHash *string
		if cc.ClientSecret != nil {
			h, err := auth.HashPassword(*cc.ClientSecret)
			if err != nil {
				return fmt.Errorf("hash client secret for %s: %w", clientID, err)
			}
			secretHash = &h
		}

		if err := repo.CreateOAuth2Client(ctx, domain.NewOAuth2Client{
			ID:               uuid.New().String(),
			ClientID:         clientID,
			ClientSecretHash: secretHash,
			RedirectURIs:     redirectURIsJSON,
			ClientName:       cc.ClientName,
			GrantTypes:       grantTypesJSON,
			Scopes:           scopesJSON,
			IsPublic:         cc.Public,
			CreatedAt:        time.Now(),
		}); err != nil {
			s.logger.Warn("oidc: seeding client (may already exist)", "client_id", clientID, "err", err)
		}
	}
	return nil
}

// withCORS lets a browser SPA on another origin (the relying party) fetch the
// provider's discovery doc, JWKS, and token endpoint. It answers preflight and
// reflects the request Origin + headers with credentials allowed — this is a
// local dev provider, so any origin is permitted. Browser-navigation parts of
// the flow (the /login and /authorize pages) are same-origin and unaffected.
func withCORS(h http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		if origin := req.Header.Get("Origin"); origin != "" {
			w.Header().Set("Access-Control-Allow-Origin", origin)
			w.Header().Add("Vary", "Origin")
			w.Header().Set("Access-Control-Allow-Credentials", "true")
		}
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		reqHeaders := req.Header.Get("Access-Control-Request-Headers")
		if reqHeaders == "" {
			reqHeaders = "Content-Type, Authorization"
		}
		w.Header().Set("Access-Control-Allow-Headers", reqHeaders)
		w.Header().Set("Access-Control-Max-Age", "86400")
		if req.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		h.ServeHTTP(w, req)
	})
}

// withDiscoveryHostRewrite makes the OIDC discovery document host-aware: it
// rewrites the *endpoint* URLs (authorization_endpoint, token_endpoint,
// jwks_uri, userinfo_endpoint, …) to the host the request came in on, while
// leaving `issuer` fixed at the configured value.
//
// This is what lets one provider serve both a host-side browser and an
// in-cluster relying party. The provider listens on all interfaces, so a pod
// reaches it via host.k3d.internal while the browser uses localhost. Without
// the rewrite, the doc bakes a single host (localhost) into jwks_uri, and a pod
// told to fetch keys from localhost hits its own loopback → "signature key not
// found". With it, the doc fetched via host.k3d.internal advertises jwks_uri on
// host.k3d.internal (pod-reachable), while issuer stays localhost so the token's
// iss still matches for both parties (the RP can point MetadataAddress at
// host.k3d.internal yet keep Authority/issuer = localhost).
func withDiscoveryHostRewrite(h http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		if req.Method != http.MethodGet || !isDiscoveryPath(req.URL.Path) {
			h.ServeHTTP(w, req)
			return
		}
		rec := &captureWriter{header: http.Header{}}
		h.ServeHTTP(rec, req)
		snap := rec.snapshot()

		rewritten, ok := rewriteDiscoveryHost(snap.body, requestOrigin(req))
		if !ok {
			// Not rewritable (non-200, non-JSON, no issuer) — pass through verbatim.
			snap.writeTo(w)
			return
		}
		for k, vs := range snap.header {
			if k == "Content-Length" {
				continue // length changed; let the writer set it
			}
			for _, v := range vs {
				w.Header().Add(k, v)
			}
		}
		w.WriteHeader(snap.status)
		_, _ = w.Write(rewritten)
	})
}

// isDiscoveryPath reports whether p is an OAuth/OIDC metadata document — the
// only responses whose endpoint hosts we rewrite.
func isDiscoveryPath(p string) bool {
	return strings.HasSuffix(p, "/.well-known/openid-configuration") ||
		strings.HasSuffix(p, "/.well-known/oauth-authorization-server")
}

// requestOrigin returns scheme://host for the request (the host the caller
// actually reached us on). Dev is plain http unless TLS is terminated here.
func requestOrigin(req *http.Request) string {
	scheme := "http"
	if req.TLS != nil {
		scheme = "https"
	}
	return scheme + "://" + req.Host
}

// rewriteDiscoveryHost rewrites every top-level URL string in a discovery
// document whose origin matches `issuer` so it instead points at newOrigin,
// leaving the `issuer` field itself untouched. Returns (body, false) unchanged
// if the body isn't a JSON object with a usable issuer.
func rewriteDiscoveryHost(body []byte, newOrigin string) ([]byte, bool) {
	var doc map[string]any
	if err := json.Unmarshal(body, &doc); err != nil {
		return body, false
	}
	issuer, _ := doc["issuer"].(string)
	if issuer == "" {
		return body, false
	}
	iss, err := url.Parse(issuer)
	if err != nil {
		return body, false
	}
	oldOrigin := iss.Scheme + "://" + iss.Host
	if newOrigin == oldOrigin {
		return body, false // same host — nothing to do, emit original bytes
	}
	for k, v := range doc {
		if k == "issuer" {
			continue
		}
		if s, ok := v.(string); ok && strings.HasPrefix(s, oldOrigin) {
			doc[k] = newOrigin + strings.TrimPrefix(s, oldOrigin)
		}
	}
	out, err := json.Marshal(doc)
	if err != nil {
		return body, false
	}
	return out, true
}

// tokenPath is the OAuth2 token endpoint on the public mux.
var tokenPath = authBasePath + "/oauth/token"

// coalesceTokenExchange makes the authorization_code grant tolerant of a relying
// party that exchanges the same code more than once. SPAs using oidc-client-ts
// frequently invoke signinRedirectCallback() repeatedly for one login — the
// callback effect re-runs across a dev double/triple-mount or HMR, firing
// several POSTs with the same code in waves milliseconds to tens of
// milliseconds apart. OAuth requires single-use codes, so the upstream
// correctly 400s every repeat after the first — but the client then treats
// sign-in as failed and bounces back to /login.
//
// We make the first successful exchange the source of truth: singleflight
// collapses a simultaneous burst into one upstream call, and the 200 result is
// cached by code for tokenReplayWindow so later waves replay the same tokens
// instead of getting a single-use 400. Only 200s are cached, so a genuinely
// invalid code still fails. This intentionally relaxes single-use enforcement
// within a short window — acceptable for this local dev provider only.
func (s *Server) coalesceTokenExchange(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		if req.Method != http.MethodPost || req.URL.Path != tokenPath {
			next.ServeHTTP(w, req)
			return
		}
		body, err := io.ReadAll(io.LimitReader(req.Body, 1<<20))
		_ = req.Body.Close()
		if err != nil {
			req.Body = io.NopCloser(bytes.NewReader(body))
			next.ServeHTTP(w, req)
			return
		}
		// Restore the body for the (possibly skipped) downstream handler.
		req.Body = io.NopCloser(bytes.NewReader(body))

		form, _ := url.ParseQuery(string(body))
		code := form.Get("code")
		if form.Get("grant_type") != "authorization_code" || code == "" {
			next.ServeHTTP(w, req)
			return
		}

		if cached, ok := s.cachedToken(code); ok {
			cached.writeTo(w)
			return
		}

		res, _, _ := s.tokenExchange.Do(code, func() (any, error) {
			if cached, ok := s.cachedToken(code); ok {
				return cached, nil
			}
			rec := &captureWriter{header: http.Header{}}
			req.Body = io.NopCloser(bytes.NewReader(body))
			next.ServeHTTP(rec, req)
			snap := rec.snapshot()
			if snap.status == http.StatusOK {
				s.storeToken(code, snap)
			}
			return snap, nil
		})
		s.tokenExchange.Forget(code)

		resp, ok := res.(capturedResponse)
		if !ok {
			next.ServeHTTP(w, req)
			return
		}
		resp.writeTo(w)
	})
}

func (s *Server) cachedToken(code string) (capturedResponse, bool) {
	s.tokenCacheMu.Lock()
	defer s.tokenCacheMu.Unlock()
	e, ok := s.tokenCache[code]
	if !ok {
		return capturedResponse{}, false
	}
	if time.Now().After(e.expiresAt) {
		delete(s.tokenCache, code)
		return capturedResponse{}, false
	}
	return e.resp, true
}

func (s *Server) storeToken(code string, resp capturedResponse) {
	now := time.Now()
	s.tokenCacheMu.Lock()
	defer s.tokenCacheMu.Unlock()
	if s.tokenCache == nil {
		s.tokenCache = make(map[string]tokenCacheEntry)
	}
	// Opportunistically drop expired entries so the map can't grow unbounded.
	for k, e := range s.tokenCache {
		if now.After(e.expiresAt) {
			delete(s.tokenCache, k)
		}
	}
	s.tokenCache[code] = tokenCacheEntry{resp: resp, expiresAt: now.Add(tokenReplayWindow)}
}

// captureWriter buffers a handler's response so it can be replayed to multiple
// coalesced callers.
type captureWriter struct {
	header http.Header
	status int
	body   bytes.Buffer
}

func (c *captureWriter) Header() http.Header { return c.header }
func (c *captureWriter) WriteHeader(status int) {
	if c.status == 0 {
		c.status = status
	}
}
func (c *captureWriter) Write(b []byte) (int, error) {
	if c.status == 0 {
		c.status = http.StatusOK
	}
	return c.body.Write(b)
}

func (c *captureWriter) snapshot() capturedResponse {
	status := c.status
	if status == 0 {
		status = http.StatusOK
	}
	return capturedResponse{status: status, header: c.header.Clone(), body: c.body.Bytes()}
}

type capturedResponse struct {
	status int
	header http.Header
	body   []byte
}

func (r capturedResponse) writeTo(w http.ResponseWriter) {
	for k, vs := range r.header {
		for _, v := range vs {
			w.Header().Add(k, v)
		}
	}
	w.WriteHeader(r.status)
	_, _ = w.Write(r.body)
}

// handleLoginPage serves the email/password sign-in page. The form posts to
// yauth's JSON login endpoint, then returns to the OIDC flow (?return=). The
// element ids (#email/#password/#submit) are a stable contract for callers'
// browser automation.
func (s *Server) handleLoginPage(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write([]byte(loginHTML))
}

// handleConsentPage is the authorization_endpoint a browser is sent to. It
// drives yauth's JSON authorize, bounces to /login when unauthenticated, and
// auto-approves consent for this local dev provider before redirecting back to
// the client with the authorization code.
func (s *Server) handleConsentPage(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write([]byte(consentHTML))
}

const pageStyle = `<style>
:root{color-scheme:light dark}
body{font:15px/1.5 system-ui,sans-serif;max-width:22rem;margin:14vh auto;padding:0 1.25rem;color:#1a1a2e}
@media(prefers-color-scheme:dark){body{color:#e8e8f0;background:#0f1020}}
.brand{font-weight:700;letter-spacing:-.02em;color:#3b82f6;margin-bottom:.25rem}
h1{font-size:1.25rem;margin:.2rem 0 1.25rem}
.muted{color:#888}
label{display:block;font-size:.8rem;font-weight:600;margin:.85rem 0 .3rem;color:#666}
@media(prefers-color-scheme:dark){label{color:#aaa}}
input{display:block;width:100%;padding:.6rem .7rem;border:1px solid #d0d0dd;border-radius:8px;box-sizing:border-box;font-size:1rem;background:transparent;color:inherit}
input:focus{outline:2px solid #3b82f6;border-color:#3b82f6}
button{margin-top:1.25rem;padding:.65rem 1rem;width:100%;cursor:pointer;border:0;border-radius:8px;background:#3b82f6;color:#fff;font-size:1rem;font-weight:600}
button:hover{background:#2563eb}
.err{color:#dc2626;min-height:1.2em;margin:.6rem 0 0;font-size:.85rem}
</style>`

// loginHTML renders the sign-in form and posts credentials to yauth's JSON
// login, then navigates back to the OIDC flow.
const loginHTML = `<!doctype html><html lang=en><head><meta charset=utf-8>
<title>Sign in · devrig</title><meta name=viewport content="width=device-width,initial-scale=1">` + pageStyle + `</head><body>
<div class=brand>devrig</div>
<h1>Sign in</h1>
<form id=f autocomplete=on>
  <label for=email>Email</label>
  <input id=email name=email type=email autocomplete=username autofocus>
  <label for=password>Password</label>
  <input id=password name=password type=password autocomplete=current-password>
  <button id=submit type=submit>Sign in</button>
  <p class=err id=err></p>
</form>
<script>
const ret=new URLSearchParams(location.search).get("return")||"/";
const f=document.getElementById("f"),err=document.getElementById("err");
f.addEventListener("submit",async e=>{
  e.preventDefault();err.textContent="";
  try{
    const res=await fetch("` + authBasePath + `/login",{method:"POST",credentials:"include",
      headers:{"content-type":"application/json"},
      body:JSON.stringify({email:document.getElementById("email").value,password:document.getElementById("password").value})});
    if(res.ok){location.href=ret;}else{err.textContent="Sign in failed ("+res.status+")";}
  }catch(ex){err.textContent="Sign in error: "+ex;}
});
</script></body></html>`

// consentHTML drives yauth's JSON authorize + consent, auto-approving for this
// local dev provider, and redirects the browser back to the client.
const consentHTML = `<!doctype html><html lang=en><head><meta charset=utf-8>
<title>Authorizing · devrig</title><meta name=viewport content="width=device-width,initial-scale=1">` + pageStyle + `</head><body>
<div class=brand>devrig</div>
<h1 id=title>Authorizing…</h1>
<p class=muted id=msg>One moment.</p>
<p class=err id=err></p>
<script>
const qs=location.search.substring(1);
const msg=document.getElementById("msg"),err=document.getElementById("err");
async function go(){
  let res;
  try{res=await fetch("` + authBasePath + `/oauth/authorize?"+qs,{credentials:"include"});}
  catch(ex){err.textContent="Authorize error: "+ex;return;}
  if(res.status===401||res.status===403){location.href="/login?return="+encodeURIComponent(location.href);return;}
  const data=await res.json().catch(()=>({}));
  if(data.redirect_url){location.href=data.redirect_url;return;}
  // Auto-approve consent for the local dev provider.
  try{
    const c=await fetch("` + authBasePath + `/oauth2/consent",{method:"POST",credentials:"include",
      headers:{"content-type":"application/json"},
      body:JSON.stringify({request_id:data.request_id,csrf_token:data.csrf_token,approved:true})});
    const out=await c.json().catch(()=>({}));
    if(out.redirect_url){location.href=out.redirect_url;}else{err.textContent="Consent failed ("+c.status+")";}
  }catch(ex){err.textContent="Consent error: "+ex;}
}
go();
</script></body></html>`

// generateRSAKeyPEM returns a fresh 2048-bit RSA keypair as PEM (private, public).
func generateRSAKeyPEM() (priv, pub []byte, err error) {
	key, err := rsa.GenerateKey(rand.Reader, 2048)
	if err != nil {
		return nil, nil, err
	}
	privPEM := pem.EncodeToMemory(&pem.Block{
		Type:  "PRIVATE KEY",
		Bytes: mustPKCS8(key),
	})
	pubDER, err := x509.MarshalPKIXPublicKey(&key.PublicKey)
	if err != nil {
		return nil, nil, err
	}
	pubPEM := pem.EncodeToMemory(&pem.Block{Type: "PUBLIC KEY", Bytes: pubDER})
	return privPEM, pubPEM, nil
}

func mustPKCS8(key *rsa.PrivateKey) []byte {
	b, err := x509.MarshalPKCS8PrivateKey(key)
	if err != nil {
		// PKCS8 marshalling of an RSA key cannot fail; fall back to PKCS1.
		return x509.MarshalPKCS1PrivateKey(key)
	}
	return b
}
