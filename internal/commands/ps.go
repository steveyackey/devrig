package commands

import (
	"fmt"
	"os"
	"text/tabwriter"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/registry"
	"github.com/steveyackey/devrig/internal/state"
)

// NewPsCmd returns the `devrig ps` command.
func NewPsCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "ps",
		Short: "List running devrig projects",
		RunE: func(cmd *cobra.Command, args []string) error {
			reg := registry.Load()
			reg.Cleanup()

			if len(reg.Instances) == 0 {
				fmt.Println("No running devrig projects.")
				return nil
			}

			w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
			fmt.Fprintln(w, "SLUG\tCONFIG\tSTARTED")
			for _, entry := range reg.Instances {
				st := state.Load(entry.StateDir)
				started := entry.StartedAt.Format("2006-01-02 15:04:05")
				if st != nil {
					started = st.StartedAt.Format("2006-01-02 15:04:05")
				}
				fmt.Fprintf(w, "%s\t%s\t%s\n", entry.Slug, entry.ConfigPath, started)
			}
			_ = w.Flush()

			return nil
		},
	}
}
