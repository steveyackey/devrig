package commands

import (
	"fmt"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/orchestrator"
)

// NewStopCmd returns the `devrig stop` command.
func NewStopCmd(globalCfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "stop",
		Short: "Stop a running devrig project",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(globalCfgFile)
			if err != nil {
				return err
			}
			orch, err := orchestrator.New(cfgPath)
			if err != nil {
				return fmt.Errorf("initializing orchestrator: %w", err)
			}
			return orch.Stop()
		},
	}
}

// NewDeleteCmd returns the `devrig delete` command.
func NewDeleteCmd(globalCfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "delete",
		Short: "Stop and remove all resources for a devrig project",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(globalCfgFile)
			if err != nil {
				return err
			}
			orch, err := orchestrator.New(cfgPath)
			if err != nil {
				return fmt.Errorf("initializing orchestrator: %w", err)
			}
			return orch.Delete()
		},
	}
}
