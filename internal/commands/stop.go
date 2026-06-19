package commands

import (
	"context"
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/cluster"
	"github.com/steveyackey/devrig/internal/docker"
	"github.com/steveyackey/devrig/internal/orchestrator"
	"github.com/steveyackey/devrig/internal/registry"
	"github.com/steveyackey/devrig/internal/tools"
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
				if err := forEachInstance("Deleting", func(o *orchestrator.Orchestrator) error {
					return o.Delete()
				}); err != nil {
					return err
				}
				// The per-instance pass above only covers projects recorded in the
				// registry. A `devrig start` interrupted before it registered (e.g.
				// Ctrl-C'd while hanging in cluster creation) can leave resources with
				// no registry entry, which the loop never sees. Reap any such
				// devrig-managed clusters, containers, volumes, and networks directly.
				reapOrphanClusters(cmd.Context())
				reapOrphanDockerResources(cmd.Context())
				return nil
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

// reapOrphanClusters deletes any devrig-managed k3d clusters that survived the
// registry-driven delete pass — i.e. clusters left behind by a start that never
// registered its instance. It is best-effort: a missing k3d binary or list
// failure is reported and ignored rather than failing the whole command.
func reapOrphanClusters(ctx context.Context) {
	// Defaults are fine here: managed-preferred resolution with on-demand fetch
	// disabled. We only need an already-present k3d to enumerate/delete; we never
	// want to download one just to clean up.
	resolver := tools.NewResolver(tools.Options{})
	names, err := cluster.ListDevrigClusters(ctx, resolver)
	if err != nil {
		fmt.Fprintf(os.Stderr, "warning: could not list k3d clusters to reap orphans: %v\n", err)
		return
	}
	for _, name := range names {
		fmt.Fprintf(os.Stderr, "  Deleting orphaned cluster %s ... ", name)
		if err := cluster.DeleteClusterByName(ctx, resolver, name); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			continue
		}
		fmt.Fprintln(os.Stderr, "done")
	}
}

// reapOrphanDockerResources removes any devrig-managed Docker containers,
// volumes, and networks that survived the registry-driven delete pass — e.g. a
// postgres container and its data volume, or a network, left behind by a start
// that never registered its instance. Best-effort: a Docker connection failure
// is reported and ignored rather than failing the whole command.
func reapOrphanDockerResources(ctx context.Context) {
	res, err := docker.ReapOrphans(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "warning: could not reap orphaned Docker resources: %v\n", err)
		return
	}
	for _, name := range res.Containers {
		fmt.Fprintf(os.Stderr, "  Removed orphaned container %s\n", name)
	}
	for _, name := range res.Volumes {
		fmt.Fprintf(os.Stderr, "  Removed orphaned volume %s\n", name)
	}
	for _, name := range res.Networks {
		fmt.Fprintf(os.Stderr, "  Removed orphaned network %s\n", name)
	}
}
