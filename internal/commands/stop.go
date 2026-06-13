package commands

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/orchestrator"
	"github.com/steveyackey/devrig/internal/registry"
)

// NewStopCmd returns the `devrig stop` command.
func NewStopCmd(globalCfgFile *string) *cobra.Command {
	var all bool
	cmd := &cobra.Command{
		Use:   "stop",
		Short: "Stop a running devrig project",
		RunE: func(cmd *cobra.Command, args []string) error {
			if all {
				return forEachInstance("Stopping", func(o *orchestrator.Orchestrator) error {
					return o.Stop()
				})
			}
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
	cmd.Flags().BoolVar(&all, "all", false, "Stop all running devrig instances across projects")
	return cmd
}

// NewDeleteCmd returns the `devrig delete` command.
func NewDeleteCmd(globalCfgFile *string) *cobra.Command {
	var all bool
	cmd := &cobra.Command{
		Use:   "delete",
		Short: "Stop and remove all resources for a devrig project",
		RunE: func(cmd *cobra.Command, args []string) error {
			if all {
				return forEachInstance("Deleting", func(o *orchestrator.Orchestrator) error {
					return o.Delete()
				})
			}
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
	cmd.Flags().BoolVar(&all, "all", false, "Delete all running devrig instances across projects")
	return cmd
}

// forEachInstance runs action against every registered devrig instance. It
// snapshots the registry first (action may mutate instances.json), skips
// entries whose config file is gone, and continues past per-instance errors so
// one failure doesn't strand the others. verb labels the per-instance output.
func forEachInstance(verb string, action func(*orchestrator.Orchestrator) error) error {
	reg := registry.Load()
	reg.Cleanup()
	_ = reg.Save()

	// Copy before iterating: action (e.g. Delete) reloads and rewrites the
	// registry, which would otherwise mutate the slice under us.
	instances := append([]registry.InstanceEntry(nil), reg.Instances...)
	if len(instances) == 0 {
		fmt.Fprintln(os.Stderr, "No running devrig instances found.")
		return nil
	}

	for _, entry := range instances {
		if _, err := os.Stat(entry.ConfigPath); err != nil {
			fmt.Fprintf(os.Stderr, "  %s — config not found, skipping\n", entry.Slug)
			continue
		}
		fmt.Fprintf(os.Stderr, "  %s %s ... ", verb, entry.Slug)
		orch, err := orchestrator.New(entry.ConfigPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			continue
		}
		if err := action(orch); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			continue
		}
		fmt.Fprintln(os.Stderr, "done")
	}
	return nil
}
