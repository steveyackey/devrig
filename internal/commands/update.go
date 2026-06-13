package commands

import (
	"archive/tar"
	"archive/zip"
	"bufio"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/spf13/cobra"
)

const (
	githubOwner = "steveyackey"
	githubRepo  = "devrig"
)

// NewUpdateCmd downloads the latest release archive from GitHub Releases and
// replaces the running binary in place. It does NOT shell out to `go install`
// (that would build without the embedded dashboard and require a Go toolchain);
// the release archives are produced by goreleaser with the dashboard baked in.
func NewUpdateCmd(currentVersion string) *cobra.Command {
	var checkOnly bool
	cmd := &cobra.Command{
		Use:   "update",
		Short: "Update devrig to the latest release",
		RunE: func(cmd *cobra.Command, args []string) error {
			rel, err := latestRelease(cmd.Context())
			if err != nil {
				return fmt.Errorf("fetching latest release: %w", err)
			}
			latest := strings.TrimPrefix(rel.TagName, "v")
			current := strings.TrimPrefix(currentVersion, "v")

			fmt.Printf("Current version: %s\nLatest version:  %s\n", current, latest)
			if current == latest && current != "dev" && current != "" {
				fmt.Println("Already up to date.")
				return nil
			}
			if checkOnly {
				fmt.Println("Run `devrig update` to install the latest version.")
				return nil
			}

			assetName := releaseAssetName(latest)
			asset := findAsset(rel.Assets, assetName)
			if asset == nil {
				return fmt.Errorf("no release asset %q found for %s/%s — install manually from https://github.com/%s/%s/releases",
					assetName, runtime.GOOS, runtime.GOARCH, githubOwner, githubRepo)
			}

			fmt.Printf("Downloading %s...\n", asset.Name)
			data, err := download(cmd.Context(), asset.BrowserDownloadURL)
			if err != nil {
				return fmt.Errorf("downloading %s: %w", asset.Name, err)
			}

			// Verify the archive against the published SHA256SUMS, when present.
			if sums := findAsset(rel.Assets, "SHA256SUMS"); sums != nil {
				sumData, err := download(cmd.Context(), sums.BrowserDownloadURL)
				if err != nil {
					return fmt.Errorf("downloading checksums: %w", err)
				}
				if err := verifyChecksum(data, asset.Name, sumData); err != nil {
					return fmt.Errorf("checksum verification failed: %w", err)
				}
				fmt.Println("Checksum verified.")
			}

			binName := "devrig"
			if runtime.GOOS == "windows" {
				binName = "devrig.exe"
			}
			newBin, err := extractBinary(data, asset.Name, binName)
			if err != nil {
				return fmt.Errorf("extracting binary: %w", err)
			}

			if err := replaceRunningBinary(newBin); err != nil {
				return fmt.Errorf("replacing binary: %w", err)
			}

			fmt.Printf("Updated devrig to %s\n", latest)
			return nil
		},
	}
	cmd.Flags().BoolVar(&checkOnly, "check", false, "Only check for a newer version; don't install")
	return cmd
}

type ghRelease struct {
	TagName string    `json:"tag_name"`
	Assets  []ghAsset `json:"assets"`
}

type ghAsset struct {
	Name               string `json:"name"`
	BrowserDownloadURL string `json:"browser_download_url"`
}

func latestRelease(ctx context.Context) (*ghRelease, error) {
	url := fmt.Sprintf("https://api.github.com/repos/%s/%s/releases/latest", githubOwner, githubRepo)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GitHub API returned %s", resp.Status)
	}
	var rel ghRelease
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return nil, err
	}
	return &rel, nil
}

// releaseAssetName mirrors the goreleaser archive name_template:
//
//	devrig_{version}_{os}_{x86_64|arch}.{tar.gz|zip}
func releaseAssetName(version string) string {
	arch := runtime.GOARCH
	if arch == "amd64" {
		arch = "x86_64"
	}
	ext := "tar.gz"
	if runtime.GOOS == "windows" {
		ext = "zip"
	}
	return fmt.Sprintf("devrig_%s_%s_%s.%s", version, runtime.GOOS, arch, ext)
}

func findAsset(assets []ghAsset, name string) *ghAsset {
	for i := range assets {
		if assets[i].Name == name {
			return &assets[i]
		}
	}
	return nil
}

func download(ctx context.Context, url string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	client := &http.Client{Timeout: 5 * time.Minute}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("download returned %s", resp.Status)
	}
	return io.ReadAll(resp.Body)
}

// verifyChecksum confirms archive's SHA256 matches its line in a SHA256SUMS
// file (format: "<hex>  <filename>" per line).
func verifyChecksum(archive []byte, name string, sums []byte) error {
	sum := sha256.Sum256(archive)
	want := hex.EncodeToString(sum[:])
	sc := bufio.NewScanner(bytes.NewReader(sums))
	for sc.Scan() {
		fields := strings.Fields(sc.Text())
		if len(fields) == 2 && fields[1] == name {
			if fields[0] == want {
				return nil
			}
			return fmt.Errorf("sha256 mismatch for %s: got %s, want %s", name, want, fields[0])
		}
	}
	return fmt.Errorf("no checksum entry for %s", name)
}

// extractBinary pulls the named binary out of a .tar.gz or .zip archive.
func extractBinary(archive []byte, archiveName, binName string) ([]byte, error) {
	if strings.HasSuffix(archiveName, ".zip") {
		return extractFromZip(archive, binName)
	}
	return extractFromTarGz(archive, binName)
}

func extractFromTarGz(archive []byte, binName string) ([]byte, error) {
	gz, err := gzip.NewReader(bytes.NewReader(archive))
	if err != nil {
		return nil, err
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, err
		}
		if filepath.Base(hdr.Name) == binName {
			return io.ReadAll(tr)
		}
	}
	return nil, fmt.Errorf("binary %q not found in archive", binName)
}

func extractFromZip(archive []byte, binName string) ([]byte, error) {
	zr, err := zip.NewReader(bytes.NewReader(archive), int64(len(archive)))
	if err != nil {
		return nil, err
	}
	for _, f := range zr.File {
		if filepath.Base(f.Name) == binName {
			rc, err := f.Open()
			if err != nil {
				return nil, err
			}
			defer rc.Close()
			return io.ReadAll(rc)
		}
	}
	return nil, fmt.Errorf("binary %q not found in archive", binName)
}

// replaceRunningBinary writes newBin over the currently-running executable.
// On Unix it writes a sibling temp file and renames over the target (atomic on
// the same filesystem, and safe while the old inode stays open). On Windows the
// running .exe can't be overwritten, so the current file is moved aside first.
func replaceRunningBinary(newBin []byte) error {
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	exe, err = filepath.EvalSymlinks(exe)
	if err != nil {
		return err
	}
	dir := filepath.Dir(exe)

	tmp, err := os.CreateTemp(dir, ".devrig-update-*")
	if err != nil {
		return fmt.Errorf("creating temp file in %s (need write permission): %w", dir, err)
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName)

	if _, err := tmp.Write(newBin); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Chmod(tmpName, 0o755); err != nil {
		return err
	}

	if runtime.GOOS == "windows" {
		old := exe + ".old"
		_ = os.Remove(old)
		if err := os.Rename(exe, old); err != nil {
			return err
		}
		if err := os.Rename(tmpName, exe); err != nil {
			_ = os.Rename(old, exe) // roll back
			return err
		}
		_ = os.Remove(old)
		return nil
	}

	return os.Rename(tmpName, exe)
}
