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
	"context"
	"crypto/rand"
	"crypto/rsa"
	"crypto/x509"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"os"
	"time"

	"github.com/google/uuid"
	yauth "github.com/yackey-labs/yauth"
	"github.com/yackey-labs/yauth/auth"
	"github.com/yackey-labs/yauth/domain"
	"github.com/yackey-labs/yauth/mcpauth"
	"github.com/yackey-labs/yauth/yauthcfg"

	"github.com/steveyackey/devrig/internal/config"
)

// authBasePath is where the yauth JSON API is mounted on the public mux.
const authBasePath = "/api/auth"

// Server is an in-process OIDC provider.
type Server struct {
	cfg    *config.OIDCConfig
	port   uint16
	logger *slog.Logger
}

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
	const privEnv, pubEnv = "DEVRIG_OIDC_SIGNING_KEY", "DEVRIG_OIDC_SIGNING_PUB"

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
	// The yauth JSON API.
	mux.Handle(authBasePath+"/", http.StripPrefix(authBasePath, y.Router()))
	// Server-rendered browser pages.
	mux.HandleFunc("/login", s.handleLoginPage)
	mux.HandleFunc("/authorize", s.handleConsentPage)

	ln, err := net.Listen("tcp", fmt.Sprintf(":%d", s.port))
	if err != nil {
		return fmt.Errorf("oidc: listen on port %d: %w", s.port, err)
	}

	srv := &http.Server{Handler: mux}
	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutCtx)
	}()

	s.logger.Info("oidc provider started", "addr", ln.Addr().String(), "issuer", issuer)
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
