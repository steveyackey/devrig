//go:build !embedspa

package dashboard

import "net/http"

// serveSPA is a no-op in dev builds — the Vite dev server (port 5173) serves
// the frontend and proxies /api + /ws to this server.
func serveSPA(w http.ResponseWriter, r *http.Request) {
	http.Error(w, "SPA not embedded — run `vp dev` or build with -tags embedspa", http.StatusNotFound)
}
