package commands

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"

	"github.com/spf13/cobra"
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

			c := exec.Command("kubectl", args...)
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
