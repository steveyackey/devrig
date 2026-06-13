package commands

import (
	"fmt"
	"os/exec"

	"github.com/spf13/cobra"
)

// NewDoctorCmd checks that all required external tools are installed.
func NewDoctorCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "doctor",
		Short: "Check required dependencies are installed",
		RunE: func(cmd *cobra.Command, args []string) error {
			type dep struct {
				name     string
				required bool
			}
			deps := []dep{
				{"docker", true},
				{"docker-compose", false},
				{"k3d", false},
				{"kubectl", false},
				{"helm", false},
			}
			ok := true
			for _, d := range deps {
				_, err := exec.LookPath(d.name)
				if err != nil {
					if d.required {
						fmt.Printf("  ✗ %-20s  MISSING (required)\n", d.name)
						ok = false
					} else {
						fmt.Printf("  - %-20s  not found (optional)\n", d.name)
					}
				} else {
					fmt.Printf("  ✓ %-20s  ok\n", d.name)
				}
			}
			if !ok {
				return fmt.Errorf("missing required dependencies")
			}
			return nil
		},
	}
}
