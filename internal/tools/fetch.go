package tools

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
)

// errUnsupportedPlatform means there is no pinned artifact for this GOOS/GOARCH.
var errUnsupportedPlatform = errors.New("unsupported platform")

func isUnsupportedPlatform(err error) bool { return errors.Is(err, errUnsupportedPlatform) }

// checksumKey is the checksum-map key for a tool/version/platform.
func checksumKey(t Tool, version, goos, goarch string) string {
	return fmt.Sprintf("%s/%s/%s/%s", t, version, goos, goarch)
}

// platformKey is checksumKey for the pinned version on the current platform.
func platformKey(t Tool) string {
	return checksumKey(t, pinnedVersion[t], runtime.GOOS, runtime.GOARCH)
}

// artifactURL returns the download URL for a tool/version/platform.
// helm ships a .tar.gz; kubectl and k3d ship a raw binary.
func artifactURL(t Tool, version, goos, goarch string) string {
	exe := ""
	if goos == "windows" {
		exe = ".exe"
	}
	switch t {
	case Kubectl:
		return fmt.Sprintf("https://dl.k8s.io/release/v%s/bin/%s/%s/kubectl%s", version, goos, goarch, exe)
	case Helm:
		return fmt.Sprintf("https://get.helm.sh/helm-v%s-%s-%s.tar.gz", version, goos, goarch)
	case K3d:
		return fmt.Sprintf("https://github.com/k3d-io/k3d/releases/download/v%s/k3d-%s-%s%s", version, goos, goarch, exe)
	}
	return ""
}

// downloadURL returns the artifact URL for the pinned version on the current platform.
func downloadURL(t Tool) string {
	return artifactURL(t, pinnedVersion[t], runtime.GOOS, runtime.GOARCH)
}

// fetch downloads, verifies, and installs the pinned managed copy of t into the
// resolver's Dir, returning its absolute path.
func (r *Resolver) fetch(ctx context.Context, t Tool) (string, error) {
	want, ok := checksums[platformKey(t)]
	if !ok {
		return "", fmt.Errorf("%s %s on %s/%s: %w", t, pinnedVersion[t], runtime.GOOS, runtime.GOARCH, errUnsupportedPlatform)
	}

	if err := os.MkdirAll(r.opts.Dir, 0o755); err != nil {
		return "", fmt.Errorf("creating %s: %w", r.opts.Dir, err)
	}

	dest := r.ManagedPath(t)
	url := downloadURL(t)
	fmt.Fprintf(r.opts.Stderr, "Fetching managed %s %s for %s/%s ...\n", t, pinnedVersion[t], runtime.GOOS, runtime.GOARCH)

	// Stage the download inside Dir so the final rename stays on one
	// filesystem (cross-device rename fails).
	staged, err := os.CreateTemp(r.opts.Dir, fmt.Sprintf(".%s-*.tmp", t))
	if err != nil {
		return "", err
	}
	stagedPath := staged.Name()
	defer os.Remove(stagedPath) // no-op once renamed away

	sum, derr := download(ctx, url, staged)
	// Release the write handle before re-opening (helm extract) or renaming —
	// Windows won't reliably allow those while the file is still open for write.
	staged.Close()
	if derr != nil {
		return "", fmt.Errorf("downloading %s: %w", url, derr)
	}
	if sum != want {
		return "", fmt.Errorf("%s checksum mismatch: got %s, want %s", t, sum, want)
	}

	// helm ships the binary inside a tarball; extract it to the final dest.
	if t == Helm {
		if err := extractHelmBinary(stagedPath, r.opts.Dir); err != nil {
			return "", err
		}
		return dest, nil
	}

	if err := os.Chmod(stagedPath, 0o755); err != nil {
		return "", err
	}
	if err := os.Rename(stagedPath, dest); err != nil {
		return "", fmt.Errorf("installing %s: %w", t, err)
	}
	return dest, nil
}

// download streams url into w and returns the hex SHA-256 of the bytes written.
func download(ctx context.Context, url string, w io.Writer) (string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("unexpected status %s", resp.Status)
	}
	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(w, h), resp.Body); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

// extractHelmBinary reads the verified helm tarball at tgzPath and writes the
// helm binary to dir as the pinned managed name, atomically.
func extractHelmBinary(tgzPath, dir string) error {
	f, err := os.Open(tgzPath)
	if err != nil {
		return err
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		return err
	}
	defer gz.Close()

	binName := "helm"
	if runtime.GOOS == "windows" {
		binName = "helm.exe"
	}
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			return fmt.Errorf("helm binary not found in archive")
		}
		if err != nil {
			return err
		}
		if hdr.Typeflag != tar.TypeReg || filepath.Base(hdr.Name) != binName {
			continue
		}
		out, err := os.CreateTemp(dir, ".helm-extract-*.tmp")
		if err != nil {
			return err
		}
		tmpName := out.Name()
		if _, err := io.Copy(out, tr); err != nil { //nolint:gosec // size bounded by verified archive
			out.Close()
			os.Remove(tmpName)
			return err
		}
		out.Close()
		if err := os.Chmod(tmpName, 0o755); err != nil {
			os.Remove(tmpName)
			return err
		}
		dest := filepath.Join(dir, managedName(Helm))
		if err := os.Rename(tmpName, dest); err != nil {
			os.Remove(tmpName)
			return err
		}
		return nil
	}
}

// managedName is the on-disk filename for a tool's pinned managed copy.
func managedName(t Tool) string {
	name := fmt.Sprintf("%s-%s", t, pinnedVersion[t])
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	return name
}

// Install fetches a tool's managed copy unconditionally (used by
// `devrig deps install`). If force is false and the pinned copy already exists,
// it is a no-op.
func (r *Resolver) Install(ctx context.Context, t Tool, force bool) (string, error) {
	dest := r.ManagedPath(t)
	if !force && fileExists(dest) {
		return dest, nil
	}
	// Temporarily allow fetch regardless of Options.AllowFetch.
	return r.fetch(ctx, t)
}

// SupportedPlatform reports whether the current GOOS/GOARCH has a pinned
// artifact for t.
func SupportedPlatform(t Tool) bool {
	_, ok := checksums[platformKey(t)]
	return ok
}
