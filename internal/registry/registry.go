// Package registry maintains the global list of running devrig instances so
// that commands like `devrig ps` and `devrig stop` can find projects without
// a config file argument.
package registry

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"
)

// InstanceEntry records a running (or recently stopped) devrig project.
type InstanceEntry struct {
	Slug       string    `json:"slug"`
	ConfigPath string    `json:"config_path"`
	StateDir   string    `json:"state_dir"`
	StartedAt  time.Time `json:"started_at"`
}

// Registry is the list stored at ~/.devrig/instances.json.
type Registry struct {
	Instances []InstanceEntry `json:"instances"`
}

func registryPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		home = "/tmp"
	}
	return filepath.Join(home, ".devrig", "instances.json")
}

// Load reads instances.json; returns an empty registry if the file doesn't exist.
func Load() *Registry {
	data, err := os.ReadFile(registryPath())
	if err != nil {
		return &Registry{}
	}
	var r Registry
	if err := json.Unmarshal(data, &r); err != nil {
		return &Registry{}
	}
	return &r
}

// Save writes instances.json atomically (tmp → rename).
func (r *Registry) Save() error {
	path := registryPath()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return err
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// Register adds or updates an entry for the given slug.
func (r *Registry) Register(entry InstanceEntry) {
	for i, e := range r.Instances {
		if e.Slug == entry.Slug {
			r.Instances[i] = entry
			return
		}
	}
	r.Instances = append(r.Instances, entry)
}

// Unregister removes the entry with the given slug.
func (r *Registry) Unregister(slug string) {
	keep := r.Instances[:0]
	for _, e := range r.Instances {
		if e.Slug != slug {
			keep = append(keep, e)
		}
	}
	r.Instances = keep
}

// Cleanup removes entries whose state.json no longer exists.
func (r *Registry) Cleanup() {
	keep := r.Instances[:0]
	for _, e := range r.Instances {
		if _, err := os.Stat(filepath.Join(e.StateDir, "state.json")); err == nil {
			keep = append(keep, e)
		}
	}
	r.Instances = keep
}
