package commands

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"
	devrig "github.com/steveyackey/devrig"
	"github.com/steveyackey/devrig/internal/config"
)

// NewSkillCmd manages the bundled Claude Code skill.
func NewSkillCmd(cfgFile *string) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "skill",
		Short: "Manage the devrig Claude Code skill",
	}

	var global bool
	install := &cobra.Command{
		Use:   "install",
		Short: "Install the devrig skill for Claude Code",
		RunE: func(cmd *cobra.Command, args []string) error {
			var target string
			if global {
				home, err := os.UserHomeDir()
				if err != nil {
					return fmt.Errorf("could not determine home directory: %w", err)
				}
				target = filepath.Join(home, ".claude", "skills", "devrig")
			} else {
				dir, err := skillConfigDir(cfgFile)
				if err != nil {
					return err
				}
				target = filepath.Join(dir, ".claude", "skills", "devrig")
			}
			if err := os.MkdirAll(target, 0o755); err != nil {
				return fmt.Errorf("creating directory %s: %w", target, err)
			}
			if err := os.WriteFile(filepath.Join(target, "SKILL.md"), []byte(devrig.SkillMD), 0o644); err != nil {
				return fmt.Errorf("writing SKILL.md to %s: %w", target, err)
			}
			fmt.Printf("Installed devrig skill to %s\n\n", target)
			fmt.Println("Try asking Claude: \"What services are running and are there any errors?\"")
			return nil
		},
	}
	install.Flags().BoolVar(&global, "global", false, "Install to ~/.claude/skills instead of the project")

	reference := &cobra.Command{
		Use:   "reference",
		Short: "Print the full configuration reference",
		RunE: func(cmd *cobra.Command, args []string) error {
			fmt.Print(devrig.SkillReferenceConfigurationMD)
			return nil
		},
	}

	cmd.AddCommand(install, reference)
	return cmd
}

// skillConfigDir returns the directory containing the config file: the -f
// flag's parent if set, else the nearest ancestor holding devrig.toml,
// falling back to the current directory.
func skillConfigDir(cfgFile *string) (string, error) {
	if cfgFile != nil && *cfgFile != "" {
		abs, err := filepath.Abs(*cfgFile)
		if err != nil {
			return "", err
		}
		return filepath.Dir(abs), nil
	}
	if path, err := config.Resolve(""); err == nil {
		return filepath.Dir(path), nil
	}
	return os.Getwd()
}
