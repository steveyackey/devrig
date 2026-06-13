package commands

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/state"
)

// NewResetCmd clears init_completed flags in state.json so init SQL reruns.
func NewResetCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "reset",
		Short: "Clear init_completed flags so init SQL runs again",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(cfgFile)
			if err != nil {
				return err
			}
			cfg, _, err := config.Load(cfgPath)
			if err != nil {
				return err
			}
			id, err := identity.New(cfg.Project.Name, cfgPath)
			if err != nil {
				return err
			}
			st := state.Load(id.StateDir)
			if st == nil {
				fmt.Println("no state found")
				return nil
			}

			for name, ds := range st.Docker {
				ds.InitCompleted = false
				ds.InitCompletedAt = nil
				st.Docker[name] = ds
			}

			data, err := json.MarshalIndent(st, "", "  ")
			if err != nil {
				return err
			}
			path := filepath.Join(id.StateDir, "state.json")
			if err := os.WriteFile(path, data, 0o644); err != nil {
				return err
			}
			fmt.Println("init_completed flags cleared")
			return nil
		},
	}
}
