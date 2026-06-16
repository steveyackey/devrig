package identity

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// ProjectIdentity holds the computed identifiers for a devrig project instance.
type ProjectIdentity struct {
	// Slug is a URL-safe identifier derived from the project name + config path hash.
	// Used in container/network/cluster names.
	Slug string
	// ConfigPath is the absolute path to devrig.toml.
	ConfigPath string
	// ProjectDir is the directory containing devrig.toml.
	ProjectDir string
	// StateDir is ~/.devrig/<slug>/ — where state.json and kubeconfig are stored.
	StateDir string
}

var slugRe = regexp.MustCompile(`[^a-z0-9]+`)

// New computes a ProjectIdentity from the project name and config path.
// The slug is: <name>-<6-char-hash> to allow multiple instances with the same
// project name (different directories).
//
// A project keeps the first slug it was ever assigned, recorded in a persistent
// index, so a future change to the slug algorithm doesn't silently re-slug the
// project and orphan its cluster/containers/state (as the Rust→Go rewrite did
// when it shortened the hash from 8 to 6 hex chars).
func New(projectName, configPath string) (*ProjectIdentity, error) {
	abs, err := filepath.Abs(configPath)
	if err != nil {
		return nil, fmt.Errorf("resolve config path: %w", err)
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("get home dir: %w", err)
	}

	slug := persistentSlug(homeDir, abs, computeSlug(projectName, abs))
	stateDir := filepath.Join(homeDir, ".devrig", slug)

	return &ProjectIdentity{
		Slug:       slug,
		ConfigPath: abs,
		ProjectDir: filepath.Dir(abs),
		StateDir:   stateDir,
	}, nil
}

// computeSlug derives the canonical slug "<name>-<6-char-hash>" from the project
// name and absolute config path.
func computeSlug(projectName, absConfigPath string) string {
	// Normalize name: lowercase, non-alphanumeric → dash, trim dashes.
	name := strings.ToLower(projectName)
	name = slugRe.ReplaceAllString(name, "-")
	name = strings.Trim(name, "-")
	if name == "" {
		name = "devrig"
	}
	if len(name) > 24 {
		name = name[:24]
	}
	name = strings.TrimRight(name, "-")

	// Hash the config path for uniqueness.
	h := sha256.Sum256([]byte(absConfigPath))
	hash := fmt.Sprintf("%x", h[:3]) // 6 hex chars

	return fmt.Sprintf("%s-%s", name, hash)
}

// slugIndexPath is the persistent map of absolute-config-path → slug.
func slugIndexPath(homeDir string) string {
	return filepath.Join(homeDir, ".devrig", "slugs.json")
}

// persistentSlug returns the slug already recorded for absConfigPath, or records
// and returns candidate if none is. It is best-effort: any I/O or parse error
// falls back to candidate, preserving the pre-index behavior.
func persistentSlug(homeDir, absConfigPath, candidate string) string {
	path := slugIndexPath(homeDir)

	index := map[string]string{}
	if data, err := os.ReadFile(path); err == nil {
		_ = json.Unmarshal(data, &index)
	}
	if existing, ok := index[absConfigPath]; ok && existing != "" {
		return existing
	}

	index[absConfigPath] = candidate
	if data, err := json.MarshalIndent(index, "", "  "); err == nil {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err == nil {
			tmp := path + ".tmp"
			if os.WriteFile(tmp, data, 0o644) == nil {
				_ = os.Rename(tmp, path)
			}
		}
	}
	return candidate
}

// EnsureStateDir creates the state directory if it does not exist.
func (id *ProjectIdentity) EnsureStateDir() error {
	return os.MkdirAll(id.StateDir, 0o755)
}
