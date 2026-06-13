package commands

import (
	"fmt"
	"os/exec"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/tools"
)

// NewDoctorCmd reports the status of the external tools devrig can use. It is
// informational and never fails: a process-only project (services + dashboard)
// runs with no external tools at all, Docker is needed only for [docker]
// services and clusters, and the cluster tools (k3d/kubectl/helm) are fetched
// on demand. So doctor reports what's available rather than gating on it.
func NewDoctorCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:          "doctor",
		Short:        "Report the status of devrig's external tools",
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			// Docker: not required for a basic (process-only) run.
			if _, err := exec.LookPath("docker"); err != nil {
				fmt.Println("  - docker                not found — needed for [docker] services and clusters; process-only services and the dashboard run without it")
			} else {
				fmt.Println("  ✓ docker                ok")
			}
			if _, err := exec.LookPath("docker-compose"); err != nil {
				fmt.Println("  - docker-compose        not found (optional)")
			} else {
				fmt.Println("  ✓ docker-compose        ok")
			}

			// Managed cluster tools: report the copy each will actually resolve
			// to (honoring the [tools] prefer setting), noting when the other
			// kind also exists. Absence is fine — devrig fetches on demand.
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

			return nil
		},
	}
}
