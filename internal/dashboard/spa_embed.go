//go:build embedspa

package dashboard

import (
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"io/fs"
	"net/http"
	"strings"
)

// dist holds the built SPA. Populated by the build step:
//   cd web && pnpm build   # or bun run build
//   cp -r web/dist internal/dashboard/dist
//   go build -tags embedspa ./...
//
//go:embed all:dist
var distFS embed.FS

var (
	spaRoot      fs.FS
	spaIndex     []byte
	spaIndexETag string
	spaServer    http.Handler
)

func init() {
	root, err := fs.Sub(distFS, "dist")
	if err != nil {
		panic("dashboard: failed to sub dist FS: " + err.Error())
	}
	index, err := fs.ReadFile(root, "index.html")
	if err != nil {
		panic("dashboard: index.html not found in dist FS: " + err.Error())
	}
	sum := sha256.Sum256(index)
	spaRoot = root
	spaIndex = index
	spaIndexETag = `"` + hex.EncodeToString(sum[:16]) + `"`
	spaServer = http.FileServer(http.FS(root))
}

func serveSPA(w http.ResponseWriter, r *http.Request) {
	serveIndex := func() {
		h := w.Header()
		h.Set("Cache-Control", "no-cache")
		h.Set("ETag", spaIndexETag)
		if r.Header.Get("If-None-Match") == spaIndexETag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		h.Set("Content-Type", "text/html; charset=utf-8")
		_, _ = w.Write(spaIndex)
	}

	p := strings.TrimPrefix(r.URL.Path, "/")
	if p == "" || p == "index.html" {
		serveIndex()
		return
	}
	f, err := spaRoot.Open(p)
	if err != nil {
		serveIndex()
		return
	}
	_ = f.Close()
	if strings.HasPrefix(p, "assets/") {
		w.Header().Set("Cache-Control", "public, max-age=31536000, immutable")
	} else {
		w.Header().Set("Cache-Control", "public, max-age=3600")
	}
	spaServer.ServeHTTP(w, r)
}
