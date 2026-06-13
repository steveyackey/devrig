package commands

import (
	"fmt"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/orchestrator"
)

// NewStartCmd returns the `devrig start` command.
func NewStartCmd(globalCfgFile *string) *cobra.Command {
	var (
		filter  []string
		devMode bool
	)

	cmd := &cobra.Command{
		Use:   "start [services...]",
		Short: "Start services defined in the config",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(globalCfgFile)
			if err != nil {
				return err
			}
			orch, err := orchestrator.New(cfgPath)
			if err != nil {
				return fmt.Errorf("initializing orchestrator: %w", err)
			}
			// Positional service names (Rust CLI parity) merge with -s flags.
			return orch.Start(append(filter, args...), devMode)
		},
	}

	cmd.Flags().StringArrayVarP(&filter, "service", "s", nil, "Start only these services (and their deps)")
	cmd.Flags().BoolVar(&devMode, "dev", false, "Start Vite dev server alongside the backend")
	// --dev is hidden from help (debug / development flag matching Rust behaviour)
	_ = cmd.Flags().MarkHidden("dev")

	return cmd
}
