package tools

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// supportedPlatforms must match scripts/update-tool-checksums.sh.
var supportedPlatforms = [][2]string{
	{"linux", "amd64"},
	{"linux", "arm64"},
	{"darwin", "amd64"},
	{"darwin", "arm64"},
	{"windows", "amd64"},
}

func TestPinnedVersionsPresent(t *testing.T) {
	for _, tool := range All {
		if pinnedVersion[tool] == "" {
			t.Errorf("no pinned version for %s", tool)
		}
	}
}

func TestChecksumsCoverEverySupportedPlatform(t *testing.T) {
	for _, tool := range All {
		v := pinnedVersion[tool]
		for _, p := range supportedPlatforms {
			key := checksumKey(tool, v, p[0], p[1])
			sum, ok := checksums[key]
			if !ok {
				t.Errorf("missing checksum for %s", key)
				continue
			}
			if len(sum) != 64 {
				t.Errorf("%s: checksum %q is not a 64-char sha256", key, sum)
			}
		}
	}
}

func TestNoStrayChecksums(t *testing.T) {
	// Every checksum key should belong to a pinned tool/version.
	for key := range checksums {
		parts := strings.SplitN(key, "/", 3)
		if len(parts) < 2 {
			t.Errorf("malformed checksum key %q", key)
			continue
		}
		tool := Tool(parts[0])
		if pinnedVersion[tool] != parts[1] {
			t.Errorf("checksum key %q does not match pinned version %q", key, pinnedVersion[tool])
		}
	}
}

func TestArtifactURL(t *testing.T) {
	cases := []struct {
		tool          Tool
		goos, goarch  string
		wantSubstring string
	}{
		{Kubectl, "linux", "amd64", "dl.k8s.io/release/v1.36.2/bin/linux/amd64/kubectl"},
		{Kubectl, "windows", "amd64", "/windows/amd64/kubectl.exe"},
		{Helm, "darwin", "arm64", "get.helm.sh/helm-v3.21.1-darwin-arm64.tar.gz"},
		{K3d, "linux", "arm64", "k3d-io/k3d/releases/download/v5.9.0/k3d-linux-arm64"},
		{K3d, "windows", "amd64", "k3d-windows-amd64.exe"},
	}
	for _, c := range cases {
		got := artifactURL(c.tool, pinnedVersion[c.tool], c.goos, c.goarch)
		if !strings.Contains(got, c.wantSubstring) {
			t.Errorf("artifactURL(%s,%s/%s) = %q, want substring %q", c.tool, c.goos, c.goarch, got, c.wantSubstring)
		}
	}
}

func TestResolverPrecedence(t *testing.T) {
	dir := t.TempDir()
	ctx := context.Background()

	// Create a fake managed kubectl at the pinned path.
	r := NewResolver(Options{Dir: dir})
	managed := r.ManagedPath(Kubectl)
	writeExec(t, managed)

	got, err := r.Path(ctx, Kubectl)
	if err != nil {
		t.Fatalf("Path: %v", err)
	}
	if got != managed {
		t.Errorf("prefer vendored: got %q, want managed %q", got, managed)
	}

	// Override beats managed.
	override := filepath.Join(t.TempDir(), "my-kubectl")
	writeExec(t, override)
	r2 := NewResolver(Options{Dir: dir, Overrides: map[Tool]string{Kubectl: override}})
	got, err = r2.Path(ctx, Kubectl)
	if err != nil {
		t.Fatalf("Path with override: %v", err)
	}
	if got != override {
		t.Errorf("override: got %q, want %q", got, override)
	}

	// Missing override is an error.
	r3 := NewResolver(Options{Dir: dir, Overrides: map[Tool]string{Kubectl: filepath.Join(dir, "nope")}})
	if _, err := r3.Path(ctx, Kubectl); err == nil {
		t.Error("expected error for missing override")
	}

	// Nothing present (empty PATH), fetch disabled → NotFoundError.
	t.Setenv("PATH", t.TempDir())
	r4 := NewResolver(Options{Dir: t.TempDir()})
	if _, err := r4.Path(ctx, Helm); err == nil {
		t.Error("expected NotFoundError when nothing present and fetch disabled")
	}
}

func TestStatus(t *testing.T) {
	dir := t.TempDir()
	r := NewResolver(Options{Dir: dir})
	writeExec(t, r.ManagedPath(K3d))

	s := r.Status(K3d)
	if s.Pinned != pinnedVersion[K3d] {
		t.Errorf("pinned: got %q", s.Pinned)
	}
	if s.ManagedPath == "" {
		t.Error("expected ManagedPath to be set")
	}
	if s.WillUse != s.ManagedPath {
		t.Errorf("prefer vendored: WillUse=%q want managed %q", s.WillUse, s.ManagedPath)
	}
}

func TestDownloadComputesChecksum(t *testing.T) {
	body := []byte("hello devrig tools")
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Write(body)
	}))
	defer srv.Close()

	var buf bytes.Buffer
	sum, err := download(context.Background(), srv.URL, &buf)
	if err != nil {
		t.Fatalf("download: %v", err)
	}
	want := sha256.Sum256(body)
	if sum != hex.EncodeToString(want[:]) {
		t.Errorf("checksum mismatch: got %s want %s", sum, hex.EncodeToString(want[:]))
	}
	if !bytes.Equal(buf.Bytes(), body) {
		t.Error("downloaded bytes differ from served body")
	}
}

func TestExtractHelmBinary(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("exec-bit semantics differ on windows")
	}
	dir := t.TempDir()
	tgz := filepath.Join(dir, "helm.tgz")
	want := []byte("#!/bin/sh\necho helm\n")
	writeHelmTarball(t, tgz, "linux-amd64/helm", want)

	if err := extractHelmBinary(tgz, dir); err != nil {
		t.Fatalf("extractHelmBinary: %v", err)
	}
	got, err := os.ReadFile(filepath.Join(dir, managedName(Helm)))
	if err != nil {
		t.Fatalf("read extracted helm: %v", err)
	}
	if !bytes.Equal(got, want) {
		t.Errorf("extracted contents differ")
	}
}

// writeExec creates an empty executable file at path.
func writeExec(t *testing.T, path string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
}

func writeHelmTarball(t *testing.T, path, entryName string, content []byte) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz := gzip.NewWriter(f)
	tw := tar.NewWriter(gz)
	hdr := &tar.Header{Name: entryName, Mode: 0o755, Size: int64(len(content)), Typeflag: tar.TypeReg}
	if err := tw.WriteHeader(hdr); err != nil {
		t.Fatal(err)
	}
	if _, err := tw.Write(content); err != nil {
		t.Fatal(err)
	}
	tw.Close()
	gz.Close()
}
