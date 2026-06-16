package commands

import (
	"fmt"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/registry"
	"github.com/steveyackey/devrig/internal/state"
	"github.com/steveyackey/devrig/internal/style"
	"github.com/steveyackey/devrig/internal/supervisor"
)

// NewPsCmd returns the `devrig ps` command.
func NewPsCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "ps",
		Short: "List devrig projects and whether they're running",
		RunE: func(cmd *cobra.Command, args []string) error {
			reg := registry.Load()
			reg.Cleanup()

			if len(reg.Instances) == 0 {
				fmt.Println("No devrig projects found.")
				return nil
			}

			type row struct {
				status, slug, cfg, started string
				running                    bool
			}
			rows := make([]row, 0, len(reg.Instances))
			slugW, cfgW := len("SLUG"), len("CONFIG")
			for _, entry := range reg.Instances {
				st := state.Load(entry.StateDir)
				started := entry.StartedAt.Format("2006-01-02 15:04:05")
				running := false
				if st != nil {
					started = st.StartedAt.Format("2006-01-02 15:04:05")
					// Running when the recorded devrig process is still alive
					// (PID + start time, so a reused PID doesn't read as running).
					running = st.PID != 0 && supervisor.SameProcess(st.PID, st.PIDStartTimeMs)
				}
				r := row{slug: entry.Slug, cfg: entry.ConfigPath, started: started, running: running}
				if len(r.slug) > slugW {
					slugW = len(r.slug)
				}
				if len(r.cfg) > cfgW {
					cfgW = len(r.cfg)
				}
				rows = append(rows, r)
			}

			// Header (gray) + rows; columns padded by visible width so colored
			// status stays aligned.
			const statusW = 7 // "running"/"stopped"
			fmt.Printf("%s  %s  %s  %s\n",
				style.Gray(style.PadRight("STATUS", statusW)),
				style.Gray(style.PadRight("SLUG", slugW)),
				style.Gray(style.PadRight("CONFIG", cfgW)),
				style.Gray("STARTED"))
			for _, r := range rows {
				status := style.Gray("stopped")
				if r.running {
					status = style.Green("running")
				}
				fmt.Printf("%s  %s  %s  %s\n",
					style.PadRight(status, statusW),
					style.PadRight(r.slug, slugW),
					style.PadRight(r.cfg, cfgW),
					r.started)
			}
			return nil
		},
	}
}
