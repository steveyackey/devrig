package commands

import "github.com/steveyackey/devrig/internal/config"

// resolveConfig returns the absolute path to the config file, using the global
// -f flag if set, otherwise walking up directories for devrig.toml/yaml.
func resolveConfig(globalCfgFile *string) (string, error) {
	return config.Resolve(*globalCfgFile)
}
