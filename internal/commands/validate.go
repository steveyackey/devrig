package commands

import (
	"fmt"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
)

func NewValidateCmd(globalCfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "validate",
		Short: "Validate the configuration file",
		RunE: func(cmd *cobra.Command, args []string) error {
			path, err := config.Resolve(*globalCfgFile)
			if err != nil {
				return err
			}
			cfg, _, err := config.Load(path)
			if err != nil {
				return err
			}
			if err := config.Validate(cfg); err != nil {
				return err
			}
			fmt.Fprintln(cmd.OutOrStdout(), "Config is valid.")
			return nil
		},
	}
}
