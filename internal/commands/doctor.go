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
			// Report the copy each tool will actually resolve to (honoring the
			// [tools] prefer setting), and note when the other kind also exists.
			r := depsResolver(cfgFile, false)
			for _, t := range tools.All {
				s := r.Status(t)
				switch s.WillUse {
				case "":
					fmt.Printf("  - %-20s  not installed — devrig will fetch managed %s on demand (or run `devrig deps install %s`)\n", t, s.Pinned, t)
				case s.OverridePath:
					fmt.Printf("  ✓ %-20s  override: %s\n", t, s.OverridePath)
				case s.ManagedPath:
					alt := ""
					if s.SystemPath != "" {
						alt = fmt.Sprintf(" (system %s also present)", s.SystemPath)
					}
					fmt.Printf("  ✓ %-20s  managed %s%s\n", t, s.Pinned, alt)
				case s.SystemPath:
					alt := ""
					if s.ManagedPath != "" {
						alt = fmt.Sprintf(" (managed %s also present)", s.Pinned)
					}
					fmt.Printf("  ✓ %-20s  system: %s%s\n", t, s.SystemPath, alt)
				}
			}

			if !ok {
				return fmt.Errorf("missing required dependencies")
			}
			return nil
		},
	}
}
