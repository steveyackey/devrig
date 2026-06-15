package commands

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/cluster"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/state"
	"github.com/steveyackey/devrig/internal/tools"
)

// NewClusterCmd returns the cluster subcommand tree.
func NewClusterCmd(cfgFile *string) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "cluster",
		Short: "Manage the k3d dev cluster",
	}
	cmd.AddCommand(
		newClusterStatusCmd(cfgFile),
		newClusterCreateCmd(cfgFile),
		newClusterKubeconfigCmd(cfgFile),
		newClusterDeleteCmd(cfgFile),
		newClusterRebuildCmd(cfgFile),
	)
	return cmd
}

func newClusterCreateCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "create",
		Short: "Create the k3d cluster (without starting services)",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, id, err := loadClusterConfig(cfgFile)
			if err != nil {
				return err
			}
			mgr := cluster.NewManager(cfg.Cluster, tools.ResolverFromConfig(cfg.Tools, true), id.Slug, id.StateDir, filepath.Dir(id.ConfigPath), "bridge")
			cs, err := mgr.Ensure(cmd.Context())
			if err != nil {
				return err
			}
			fmt.Printf("cluster %s ready (kubeconfig: %s)\n", cs.ClusterName, cs.KubeconfigPath)
			return nil
		},
	}
}

func newClusterRebuildCmd(cfgFile *string) *cobra.Command {
	var noApply bool
	cmd := &cobra.Command{
		Use:   "rebuild [images...]",
		Short: "Rebuild and re-push cluster images (all, or just the named ones)",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfg, id, err := loadClusterConfig(cfgFile)
			if err != nil {
				return err
			}
			st := state.Load(id.StateDir)
			if st == nil || st.Cluster == nil {
				return fmt.Errorf("no cluster state found — is devrig running?")
			}
			cs := st.Cluster

			selected := func(name string) bool {
				if len(args) == 0 {
					return true
				}
				for _, a := range args {
					if a == name {
						return true
					}
				}
				return false
			}

			configDir := filepath.Dir(id.ConfigPath)
			imgOrder, err := cluster.ImageBuildOrder(cfg.Cluster.Images)
			if err != nil {
				return err
			}
			for _, name := range imgOrder {
				if !selected(name) {
					continue
				}
				ic := cfg.Cluster.Images[name]
				tag, err := cluster.BuildImage(cmd.Context(), name, &ic, cs, configDir)
				if err != nil {
					return fmt.Errorf("rebuild %s: %w", name, err)
				}
				fmt.Printf("rebuilt %s → %s\n", name, tag)
			}
			for name, depCfg := range cfg.Cluster.Deploy {
				if !selected(name) {
					continue
				}
				dc := depCfg
				if noApply {
					dc.Manifests = ""
				}
				if err := cluster.BuildAndDeploy(cmd.Context(), tools.ResolverFromConfig(cfg.Tools, true), name, &dc, cs, id.StateDir, configDir); err != nil {
					return fmt.Errorf("rebuild %s: %w", name, err)
				}
				fmt.Printf("rebuilt and deployed %s\n", name)
			}
			return st.Save(id.StateDir)
		},
	}
	cmd.Flags().BoolVar(&noApply, "no-apply", false, "Rebuild images without re-applying manifests")
	return cmd
}

// loadClusterConfig loads the config and identity, requiring a [cluster] block.
func loadClusterConfig(cfgFile *string) (*config.Config, *identity.ProjectIdentity, error) {
	cfgPath, err := resolveConfig(cfgFile)
	if err != nil {
		return nil, nil, err
	}
	cfg, _, err := config.Load(cfgPath)
	if err != nil {
		return nil, nil, err
	}
	if cfg.Cluster == nil {
		return nil, nil, fmt.Errorf("no [cluster] block in config")
	}
	id, err := identity.New(cfg.Project.Name, cfgPath)
	if err != nil {
		return nil, nil, err
	}
	return cfg, id, nil
}

func newClusterStatusCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show cluster status from state",
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
			if st == nil || st.Cluster == nil {
				fmt.Println("no cluster state found")
				return nil
			}
			enc := json.NewEncoder(os.Stdout)
			enc.SetIndent("", "  ")
			return enc.Encode(st.Cluster)
		},
	}
}

func newClusterKubeconfigCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "kubeconfig",
		Short: "Print the path to the cluster kubeconfig",
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
			if st == nil || st.Cluster == nil {
				return fmt.Errorf("no cluster state found")
			}
			fmt.Println(st.Cluster.KubeconfigPath)
			return nil
		},
	}
}

func newClusterDeleteCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "delete",
		Short: "Delete the k3d cluster",
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(cfgFile)
			if err != nil {
				return err
			}
			cfg, _, err := config.Load(cfgPath)
			if err != nil {
				return err
			}
			if cfg.Cluster == nil {
				return fmt.Errorf("no [cluster] block in config")
			}
			id, err := identity.New(cfg.Project.Name, cfgPath)
			if err != nil {
				return err
			}
			mgr := cluster.NewManager(cfg.Cluster, tools.ResolverFromConfig(cfg.Tools, false), id.Slug, id.StateDir, filepath.Dir(id.ConfigPath), "")
			if err := mgr.Delete(cmd.Context()); err != nil {
				return err
			}
			fmt.Println("cluster deleted")
			return nil
		},
	}
}
