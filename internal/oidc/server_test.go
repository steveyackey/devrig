package oidc

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"net"
	"net/http"
	"net/http/cookiejar"
	"net/url"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/steveyackey/devrig/internal/config"
)

// freePort returns an available TCP port.
func freePort(t *testing.T) uint16 {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	return uint16(l.Addr().(*net.TCPAddr).Port)
}

func strp(s string) *string { return &s }

// TestBrowserOIDCFlow exercises the full server-rendered browser OIDC flow that
// downstream apps (and their Playwright suites) rely on: discovery, JWKS, the
// /login + /authorize pages, JSON login, auto-approved consent, the code
// exchange, and RS256-signed id_tokens.
func TestBrowserOIDCFlow(t *testing.T) {
	port := freePort(t)
	base := "http://localhost:" + itoa(port)
	const redirect = "http://localhost:9999/callback"

	cfg := &config.OIDCConfig{
		Realm: "theoven",
		Users: []config.OIDCUserConfig{
			{Email: "admin@theoven.dev", Password: "admin", Name: strp("Admin"), Role: strp("admin")},
		},
		Clients: map[string]config.OIDCClientConfig{
			"theoven-web": {Public: true, RedirectURIs: []string{redirect}, ClientName: strp("TheOven")},
		},
	}

	srv := New(cfg, port, nil)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go func() { _ = srv.Start(ctx) }()

	// Wait for the provider to come up.
	disco := mustGetUp(t, base+"/.well-known/openid-configuration", 10*time.Second)

	// Discovery: issuer at root, authorization_endpoint at the consent page,
	// token + jwks under /api/auth.
	if disco["issuer"] != base {
		t.Errorf("issuer = %v, want %s", disco["issuer"], base)
	}
	if ae, _ := disco["authorization_endpoint"].(string); !strings.HasSuffix(ae, "/authorize") {
		t.Errorf("authorization_endpoint = %v, want .../authorize", disco["authorization_endpoint"])
	}
	if te, _ := disco["token_endpoint"].(string); !strings.Contains(te, authBasePath+"/oauth/token") {
		t.Errorf("token_endpoint = %v, want %s/oauth/token", disco["token_endpoint"], authBasePath)
	}
	jwksURI, _ := disco["jwks_uri"].(string)
	if jwksURI == "" {
		t.Fatal("no jwks_uri in discovery")
	}

	// JWKS must publish at least one RSA key.
	jwks := getJSON(t, jwksURI)
	keys, _ := jwks["keys"].([]any)
	if len(keys) == 0 {
		t.Fatal("jwks has no keys — id_tokens won't be verifiable")
	}

	// The /login page must expose the selectors downstream Playwright drives.
	loginHTML := getText(t, base+"/login")
	for _, sel := range []string{`id=email`, `id=password`, `id=submit`} {
		if !strings.Contains(loginHTML, sel) {
			t.Errorf("/login page missing selector %q", sel)
		}
	}

	// --- drive the browser flow over the JSON API with a cookie jar ---
	jar, _ := cookiejar.New(nil)
	client := &http.Client{Jar: jar, Timeout: 10 * time.Second}

	// 1. Login (what the /login page POSTs).
	login := postJSON(t, client, base+authBasePath+"/login",
		map[string]string{"email": "admin@theoven.dev", "password": "admin"})
	if login.StatusCode != 200 {
		t.Fatalf("login status = %d, want 200", login.StatusCode)
	}
	login.Body.Close()

	// 2. Authorize with PKCE (what the /authorize page GETs).
	verifier := "devrig-test-verifier-0123456789-0123456789-0123456789"
	sum := sha256.Sum256([]byte(verifier))
	challenge := base64.RawURLEncoding.EncodeToString(sum[:])
	q := url.Values{
		"client_id":             {"theoven-web"},
		"redirect_uri":          {redirect},
		"response_type":         {"code"},
		"scope":                 {"openid profile email"},
		"state":                 {"xyz"},
		"nonce":                 {"n0nce"},
		"code_challenge":        {challenge},
		"code_challenge_method": {"S256"},
	}
	authzResp := getJSONClient(t, client, base+authBasePath+"/oauth/authorize?"+q.Encode())

	// 3. Get the redirect (auto-approve consent if prompted).
	redirectURL, _ := authzResp["redirect_url"].(string)
	if redirectURL == "" {
		reqID, _ := authzResp["request_id"].(string)
		csrf, _ := authzResp["csrf_token"].(string)
		if reqID == "" {
			t.Fatalf("authorize returned neither redirect_url nor a consent payload: %v", authzResp)
		}
		consent := postJSON(t, client, base+authBasePath+"/oauth2/consent",
			map[string]any{"request_id": reqID, "csrf_token": csrf, "approved": true})
		var out map[string]any
		json.NewDecoder(consent.Body).Decode(&out)
		consent.Body.Close()
		redirectURL, _ = out["redirect_url"].(string)
	}
	if redirectURL == "" {
		t.Fatal("no redirect_url after authorize/consent")
	}

	// 4. Extract the authorization code.
	ru, err := url.Parse(redirectURL)
	if err != nil {
		t.Fatal(err)
	}
	code := ru.Query().Get("code")
	if code == "" {
		t.Fatalf("no code in redirect %q", redirectURL)
	}

	// 5. Exchange the code for tokens.
	form := url.Values{
		"grant_type":    {"authorization_code"},
		"code":          {code},
		"redirect_uri":  {redirect},
		"client_id":     {"theoven-web"},
		"code_verifier": {verifier},
	}
	tokResp, err := client.PostForm(base+authBasePath+"/oauth/token", form)
	if err != nil {
		t.Fatal(err)
	}
	var tok map[string]any
	json.NewDecoder(tokResp.Body).Decode(&tok)
	tokResp.Body.Close()
	if tokResp.StatusCode != 200 {
		t.Fatalf("token status = %d, body=%v", tokResp.StatusCode, tok)
	}
	if _, ok := tok["access_token"].(string); !ok {
		t.Fatalf("no access_token: %v", tok)
	}
	idToken, _ := tok["id_token"].(string)
	if idToken == "" {
		t.Fatal("no id_token")
	}
	// id_token must be RS256-signed (asymmetric) so the RP can verify it.
	if alg := jwtAlg(t, idToken); alg != "RS256" {
		t.Errorf("id_token alg = %q, want RS256", alg)
	}
}

// --- helpers ---

func itoa(p uint16) string {
	return strconv.FormatUint(uint64(p), 10)
}

func mustGetUp(t *testing.T, url string, within time.Duration) map[string]any {
	t.Helper()
	deadline := time.Now().Add(within)
	for time.Now().Before(deadline) {
		resp, err := http.Get(url) //nolint:gosec
		if err == nil && resp.StatusCode == 200 {
			var m map[string]any
			json.NewDecoder(resp.Body).Decode(&m)
			resp.Body.Close()
			return m
		}
		if resp != nil {
			resp.Body.Close()
		}
		time.Sleep(100 * time.Millisecond)
	}
	t.Fatalf("provider never came up at %s", url)
	return nil
}

func getJSON(t *testing.T, url string) map[string]any {
	t.Helper()
	resp, err := http.Get(url) //nolint:gosec
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var m map[string]any
	json.NewDecoder(resp.Body).Decode(&m)
	return m
}

func getJSONClient(t *testing.T, c *http.Client, url string) map[string]any {
	t.Helper()
	resp, err := c.Get(url)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == 401 || resp.StatusCode == 403 {
		t.Fatalf("authorize returned %d — session cookie from /login not honored", resp.StatusCode)
	}
	var m map[string]any
	json.NewDecoder(resp.Body).Decode(&m)
	return m
}

func postJSON(t *testing.T, c *http.Client, url string, body any) *http.Response {
	t.Helper()
	b, _ := json.Marshal(body)
	resp, err := c.Post(url, "application/json", strings.NewReader(string(b)))
	if err != nil {
		t.Fatal(err)
	}
	return resp
}

func getText(t *testing.T, url string) string {
	t.Helper()
	resp, err := http.Get(url) //nolint:gosec
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	b := make([]byte, 8192)
	n, _ := resp.Body.Read(b)
	return string(b[:n])
}

func jwtAlg(t *testing.T, token string) string {
	t.Helper()
	parts := strings.Split(token, ".")
	if len(parts) < 2 {
		t.Fatalf("malformed jwt")
	}
	hdr, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		t.Fatalf("decode jwt header: %v", err)
	}
	var h map[string]any
	json.Unmarshal(hdr, &h)
	alg, _ := h["alg"].(string)
	return alg
}
