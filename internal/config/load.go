package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
	"gopkg.in/yaml.v3"
)

// Load reads, parses, and validates a devrig config file (TOML or YAML).
// It does not perform {{ }} template interpolation — that happens later after
// ports are resolved. It does expand $VAR references from .env files.
func Load(path string) (*Config, *SecretRegistry, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, fmt.Errorf("read config: %w", err)
	}

	var cfg Config
	if err := decode(path, data, &cfg); err != nil {
		return nil, nil, fmt.Errorf("parse config: %w", err)
	}

	// Apply defaults for maps that may be nil after decode.
	if cfg.Services == nil {
		cfg.Services = make(map[string]ServiceConfig)
	}
	if cfg.Docker == nil {
		cfg.Docker = make(map[string]DockerConfig)
	}
	if cfg.Env == nil {
		cfg.Env = make(map[string]string)
	}
	if cfg.Links == nil {
		cfg.Links = make(map[string]string)
	}

	reg := &SecretRegistry{}

	// Load project-level .env file.
	envFileVars := make(map[string]string)
	if cfg.Project.EnvFile != nil {
		envPath := *cfg.Project.EnvFile
		if !filepath.IsAbs(envPath) {
			envPath = filepath.Join(filepath.Dir(path), envPath)
		}
		vars, err := ParseEnvFile(envPath)
		if err != nil {
			return nil, nil, fmt.Errorf("load env_file: %w", err)
		}
		for k, v := range vars {
			reg.Track(v)
			envFileVars[k] = v
		}
	}

	// Expand $VAR references throughout the config.
	if err := ExpandConfigEnvVars(&cfg, envFileVars, reg); err != nil {
		return nil, nil, err
	}

	return &cfg, reg, nil
}

// decode routes to the correct unmarshaler based on file extension.
func decode(path string, data []byte, cfg *Config) error {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".yaml", ".yml":
		if err := yaml.Unmarshal(data, cfg); err != nil {
			return err
		}
		return nil
	default:
		// Default to TOML for .toml and unrecognized extensions.
		if _, err := toml.Decode(string(data), cfg); err != nil {
			return err
		}
		return nil
	}
}
