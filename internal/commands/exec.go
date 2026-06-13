package commands

import (
	"fmt"
	"os"
	"os/exec"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/state"
)

// NewExecCmd runs a command inside a managed Docker container.
func NewExecCmd(cfgFile *string) *cobra.Command {
	var interactive bool
	cmd := &cobra.Command{
		Use:   "exec <service> [command...]",
		Short: "Execute a command in a running Docker container",
		Args:  cobra.MinimumNArgs(1),
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
				return fmt.Errorf("devrig is not running")
			}

			svcName := args[0]
			ds, ok := st.Docker[svcName]
			if !ok || ds.ContainerName == "" {
				return fmt.Errorf("no running Docker container for service %q", svcName)
			}

			dockerArgs := []string{"exec"}
			if interactive {
				dockerArgs = append(dockerArgs, "-it")
			}
			dockerArgs = append(dockerArgs, ds.ContainerName)
			if len(args) > 1 {
				dockerArgs = append(dockerArgs, args[1:]...)
			} else {
				dockerArgs = append(dockerArgs, "/bin/sh")
			}

			c := exec.Command("docker", dockerArgs...)
			c.Stdin = os.Stdin
			c.Stdout = os.Stdout
			c.Stderr = os.Stderr
			return c.Run()
		},
	}
	cmd.Flags().BoolVarP(&interactive, "interactive", "i", true, "Attach stdin/tty")
	return cmd
}
