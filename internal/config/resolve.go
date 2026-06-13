package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

// candidateNames are the config filenames searched in order when no -f flag
// is given. TOML is checked first for backwards compatibility.
var candidateNames = []string{
	"devrig.toml",
	"devrig.yaml",
	"devrig.yml",
}

// Resolve finds the config file to use. If path is provided it is verified to
// exist. Otherwise it walks up the directory tree from the current working
// directory until it finds one of the candidate filenames.
func Resolve(path string) (string, error) {
	if path != "" {
		abs, err := filepath.Abs(path)
		if err != nil {
			return "", err
		}
		if _, err := os.Stat(abs); err != nil {
			return "", fmt.Errorf("config file not found: %s", abs)
		}
		return abs, nil
	}

	cwd, err := os.Getwd()
	if err != nil {
		return "", err
	}
	found := walkUp(cwd)
	if found == "" {
		return "", errors.New("no devrig.toml (or devrig.yaml) found (searched current directory and parents)")
	}
	return found, nil
}

func walkUp(start string) string {
	dir := start
	for {
		for _, name := range candidateNames {
			candidate := filepath.Join(dir, name)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}
