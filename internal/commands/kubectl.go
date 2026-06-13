package commands

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/tools"
)

// NewKubectlCmd proxies to kubectl with devrig's isolated kubeconfig.
func NewKubectlCmd(cfgFile *string) *cobra.Command {
	cmd := &cobra.Command{
		Use:                "kubectl [args...]",
		Aliases:            []string{"k"},
		Short:              "Proxy to kubectl with devrig's isolated kubeconfig",
		DisableFlagParsing: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			cfgPath, err := resolveConfig(cfgFile)
			if err != nil {
				return err
			}
			kubeconfig := filepath.Join(filepath.Dir(cfgPath), ".devrig", "kubeconfig")
			if _, err := os.Stat(kubeconfig); err != nil {
				return fmt.Errorf("kubeconfig not found — is the cluster running? Start with `devrig start` first")
			}

			// The passthrough deliberately prefers the user's own kubectl (krew
			// plugins, muscle memory) over the managed copy, regardless of the
			// global [tools] prefer setting — but still honors an explicit
			// override and falls back to a managed fetch if none is installed.
			opts := tools.Options{Prefer: tools.PreferSystem, AllowFetch: true, Overrides: map[tools.Tool]string{}}
			if cfg, _, lerr := config.Load(cfgPath); lerr == nil && cfg.Tools != nil && cfg.Tools.Kubectl != "" {
				opts.Overrides[tools.Kubectl] = cfg.Tools.Kubectl
			}
			bin, err := tools.NewResolver(opts).Path(cmd.Context(), tools.Kubectl)
			if err != nil {
				return err
			}

			c := exec.Command(bin, args...)
			c.Env = append(os.Environ(), "KUBECONFIG="+kubeconfig)
			c.Stdin = os.Stdin
			c.Stdout = os.Stdout
			c.Stderr = os.Stderr
			if err := c.Run(); err != nil {
				if exitErr, ok := err.(*exec.ExitError); ok {
					os.Exit(exitErr.ExitCode())
				}
				return fmt.Errorf("running kubectl: %w", err)
			}
			return nil
		},
	}
	return cmd
}
