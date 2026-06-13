package commands

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"os"

	"github.com/spf13/cobra"
)

// NewQueryCmd queries OTel telemetry from a running devrig instance.
func NewQueryCmd(cfgFile *string) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "query",
		Short: "Query OTel telemetry from the running devrig instance",
	}
	cmd.AddCommand(
		newQueryTracesCmd(cfgFile),
		newQueryTraceCmd(cfgFile),
		newQueryLogsCmd(cfgFile),
		newQueryMetricsCmd(cfgFile),
		newQueryStatusCmd(cfgFile),
		newQueryRelatedCmd(cfgFile),
	)
	return cmd
}

func newQueryTraceCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "trace <trace-id>",
		Short: "Show a single trace with all its spans",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/traces/"+url.PathEscape(args[0]), url.Values{})
		},
	}
}

func newQueryStatusCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show telemetry store counts",
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/status", url.Values{})
		},
	}
}

func newQueryRelatedCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "related <trace-id>",
		Short: "Show logs and metrics related to a trace",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/traces/"+url.PathEscape(args[0])+"/related", url.Values{})
		},
	}
}

func newQueryTracesCmd(cfgFile *string) *cobra.Command {
	var service, search string
	var limit int
	cmd := &cobra.Command{
		Use:   "traces",
		Short: "List recent traces",
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/traces", url.Values{
				"service": {service},
				"search":  {search},
				"limit":   {fmt.Sprint(limit)},
			})
		},
	}
	cmd.Flags().StringVarP(&service, "service", "s", "", "Filter by service")
	cmd.Flags().StringVarP(&search, "search", "q", "", "Search operation name")
	cmd.Flags().IntVarP(&limit, "limit", "n", 20, "Max results")
	return cmd
}

func newQueryLogsCmd(cfgFile *string) *cobra.Command {
	var service, severity, search string
	var limit int
	cmd := &cobra.Command{
		Use:   "logs",
		Short: "List recent log records",
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/logs", url.Values{
				"service":  {service},
				"severity": {severity},
				"search":   {search},
				"limit":    {fmt.Sprint(limit)},
			})
		},
	}
	cmd.Flags().StringVarP(&service, "service", "s", "", "Filter by service")
	cmd.Flags().StringVar(&severity, "severity", "", "Filter by severity")
	cmd.Flags().StringVarP(&search, "search", "q", "", "Search log body")
	cmd.Flags().IntVarP(&limit, "limit", "n", 50, "Max results")
	return cmd
}

func newQueryMetricsCmd(cfgFile *string) *cobra.Command {
	var service, name string
	var limit int
	cmd := &cobra.Command{
		Use:   "metrics",
		Short: "List recent metric points",
		RunE: func(cmd *cobra.Command, args []string) error {
			return queryAPI(cfgFile, "/api/metrics", url.Values{
				"service":     {service},
				"metric_name": {name},
				"limit":       {fmt.Sprint(limit)},
			})
		},
	}
	cmd.Flags().StringVarP(&service, "service", "s", "", "Filter by service")
	cmd.Flags().StringVarP(&name, "name", "n", "", "Filter by metric name")
	cmd.Flags().IntVarP(&limit, "limit", "l", 50, "Max results")
	return cmd
}

func queryAPI(cfgFile *string, path string, params url.Values) error {
	port, err := dashboardPort(cfgFile)
	if err != nil {
		return err
	}

	q := url.Values{}
	for k, v := range params {
		if len(v) > 0 && v[0] != "" {
			q.Set(k, v[0])
		}
	}
	apiURL := fmt.Sprintf("http://localhost:%d%s?%s", port, path, q.Encode())

	resp, err := http.Get(apiURL)
	if err != nil {
		return fmt.Errorf("connecting to devrig dashboard: %w", err)
	}
	defer resp.Body.Close()

	var data any
	if err := json.NewDecoder(resp.Body).Decode(&data); err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(data)
}
