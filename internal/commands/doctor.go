package commands

import (
	"fmt"
	"os/exec"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/tools"
)

// NewDoctorCmd checks that required external tools are installed. The
// cluster tools (k3d/kubectl/helm) are reported via the tools resolver, since
// devrig can fetch managed copies of them on demand.
func NewDoctorCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Check required dependencies are installed",
		RunE: func(cmd *cobra.Command, args []string) error {
			ok := true

			// Plain PATH checks for tools devrig does not manage.
			for _, d := range []struct {
				name     string
				required bool
			}{
				{"docker", true},
				{"docker-compose", false},
			} {
				if _, err := exec.LookPath(d.name); err != nil {
					if d.required {
						fmt.Printf("  ✗ %-20s  MISSING (required)\n", d.name)
						ok = false
					} else {
						fmt.Printf("  - %-20s  not found (optional)\n", d.name)
					}
					continue
				}
				fmt.Printf("  ✓ %-20s  ok\n", d.name)
			}

			// Managed cluster tools: show managed/system status. Absence is not
			// a failure — devrig fetches a pinned copy on demand.
			r := depsResolver(cfgFile, false)
			for _, t := range tools.All {
				s := r.Status(t)
				switch {
				case s.OverridePath != "":
					fmt.Printf("  ✓ %-20s  override: %s\n", t, s.OverridePath)
				case s.ManagedPath != "":
					fmt.Printf("  ✓ %-20s  managed %s\n", t, s.Pinned)
				case s.SystemPath != "":
					fmt.Printf("  ✓ %-20s  system: %s\n", t, s.SystemPath)
				default:
					fmt.Printf("  - %-20s  not installed — devrig will fetch managed %s on demand (or run `devrig deps install %s`)\n", t, s.Pinned, t)
				}
			}

			if !ok {
				return fmt.Errorf("missing required dependencies")
			}
			return nil
		},
	}
}
