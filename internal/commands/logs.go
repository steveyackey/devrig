package commands

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"regexp"
	"time"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/state"
)

// NewLogsCmd queries stored logs from the running devrig dashboard.
func NewLogsCmd(cfgFile *string) *cobra.Command {
	var service, severity, search, exclude, since string
	var limit int
	var follow, timestamps bool

	cmd := &cobra.Command{
		Use:   "logs [services...]",
		Short: "Query stored logs from the running devrig instance",
		RunE: func(cmd *cobra.Command, args []string) error {
			dashPort, err := dashboardPort(cfgFile)
			if err != nil {
				return err
			}
			// Positional service arg (Rust CLI parity).
			if service == "" && len(args) > 0 {
				service = args[0]
			}

			var excludeRe *regexp.Regexp
			if exclude != "" {
				excludeRe, err = regexp.Compile(exclude)
				if err != nil {
					return fmt.Errorf("invalid --exclude pattern: %w", err)
				}
			}

			base := fmt.Sprintf("http://localhost:%d/api/logs", dashPort)
			q := url.Values{}
			if service != "" {
				q.Set("service", service)
			}
			if severity != "" {
				q.Set("severity", severity)
			}
			if search != "" {
				q.Set("search", search)
			}
			if since != "" {
				if d, derr := time.ParseDuration(since); derr == nil {
					q.Set("since", time.Now().Add(-d).UTC().Format(time.RFC3339))
				} else {
					q.Set("since", since) // assume RFC3339
				}
			}
			q.Set("limit", fmt.Sprint(limit))

			poll := func() error {
				resp, err := http.Get(base + "?" + q.Encode())
				if err != nil {
					return fmt.Errorf("connecting to devrig dashboard: %w", err)
				}
				defer resp.Body.Close()

				var logs []map[string]any
				if err := json.NewDecoder(resp.Body).Decode(&logs); err != nil {
					return err
				}
				for _, l := range logs {
					ts, _ := l["timestamp"].(string)
					svc, _ := l["service_name"].(string)
					sev, _ := l["severity"].(string)
					body, _ := l["body"].(string)
					if excludeRe != nil && excludeRe.MatchString(body) {
						continue
					}
					if timestamps {
						fmt.Printf("[%s] [%s] [%s] %s\n", ts, svc, sev, body)
					} else {
						fmt.Printf("[%s] [%s] %s\n", svc, sev, body)
					}
				}
				return nil
			}

			if !follow {
				return poll()
			}

			for {
				_ = poll()
				time.Sleep(2 * time.Second)
			}
		},
	}

	cmd.Flags().StringVarP(&service, "service", "s", "", "Filter by service name")
	cmd.Flags().StringVar(&severity, "severity", "", "Filter by severity (debug|info|warn|error)")
	cmd.Flags().StringVar(&severity, "level", "", "Alias for --severity")
	cmd.Flags().StringVarP(&search, "grep", "g", "", "Search log body")
	cmd.Flags().StringVar(&exclude, "exclude", "", "Exclude lines matching this regex")
	cmd.Flags().StringVar(&since, "since", "", "Only logs newer than a duration (5m) or RFC3339 time")
	cmd.Flags().IntVarP(&limit, "limit", "n", 100, "Maximum number of log records")
	cmd.Flags().IntVar(&limit, "tail", 100, "Alias for --limit")
	cmd.Flags().BoolVarP(&follow, "follow", "f", false, "Poll for new logs every 2s")
	cmd.Flags().BoolVarP(&timestamps, "timestamps", "t", false, "Show timestamps")
	return cmd
}

// dashboardPort resolves the running devrig dashboard port from state.
func dashboardPort(cfgFile *string) (uint16, error) {
	cfgPath, err := config.Resolve("")
	if cfgFile != nil && *cfgFile != "" {
		cfgPath, err = config.Resolve(*cfgFile)
	}
	if err != nil {
		return 0, err
	}
	cfg, _, err := config.Load(cfgPath)
	if err != nil {
		return 0, err
	}
	id, err := identity.New(cfg.Project.Name, cfgPath)
	if err != nil {
		return 0, err
	}
	st := state.Load(id.StateDir)
	if st == nil || st.Dashboard == nil {
		return 0, fmt.Errorf("devrig is not running (no state found)")
	}
	return st.Dashboard.DashboardPort, nil
}
