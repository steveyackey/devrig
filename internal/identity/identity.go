package identity

import (
	"crypto/sha256"
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
func New(projectName, configPath string) (*ProjectIdentity, error) {
	abs, err := filepath.Abs(configPath)
	if err != nil {
		return nil, fmt.Errorf("resolve config path: %w", err)
	}

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
	h := sha256.Sum256([]byte(abs))
	hash := fmt.Sprintf("%x", h[:3]) // 6 hex chars

	slug := fmt.Sprintf("%s-%s", name, hash)

	homeDir, err := os.UserHomeDir()
	if err != nil {
		return nil, fmt.Errorf("get home dir: %w", err)
	}
	stateDir := filepath.Join(homeDir, ".devrig", slug)

	return &ProjectIdentity{
		Slug:       slug,
		ConfigPath: abs,
		ProjectDir: filepath.Dir(abs),
		StateDir:   stateDir,
	}, nil
}

// EnsureStateDir creates the state directory if it does not exist.
func (id *ProjectIdentity) EnsureStateDir() error {
	return os.MkdirAll(id.StateDir, 0o755)
}
