package commands

import (
	"fmt"
	"os"
	"text/tabwriter"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/tools"
)

// NewDepsCmd manages devrig's managed external tool binaries (k3d/kubectl/helm).
func NewDepsCmd(cfgFile *string) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "deps",
		Short: "Manage devrig-managed tool binaries (k3d, kubectl, helm)",
	}
	cmd.AddCommand(
		newDepsListCmd(cfgFile),
		newDepsInstallCmd(cfgFile, false),
		newDepsInstallCmd(cfgFile, true),
	)
	return cmd
}

// depsResolver builds a resolver from the project's [tools] config if a config
// file can be found, otherwise from defaults. allowFetch controls downloads.
func depsResolver(cfgFile *string, allowFetch bool) *tools.Resolver {
	var toolsCfg *config.ToolsConfig
	if cfgPath, err := resolveConfig(cfgFile); err == nil {
		if cfg, _, lerr := config.Load(cfgPath); lerr == nil {
			toolsCfg = cfg.Tools
		}
	}
	return tools.ResolverFromConfig(toolsCfg, allowFetch)
}

// parseToolArgs maps positional args to Tools, defaulting to all when empty.
func parseToolArgs(args []string) ([]tools.Tool, error) {
	if len(args) == 0 {
		return tools.All, nil
	}
	var out []tools.Tool
	for _, a := range args {
		switch tools.Tool(a) {
		case tools.Kubectl, tools.Helm, tools.K3d:
			out = append(out, tools.Tool(a))
		default:
			return nil, fmt.Errorf("unknown tool %q (kubectl|helm|k3d)", a)
		}
	}
	return out, nil
}

func newDepsListCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "Show pinned versions and which copy devrig will use",
		RunE: func(cmd *cobra.Command, args []string) error {
			r := depsResolver(cfgFile, false)
			w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
			fmt.Fprintln(w, "TOOL\tPINNED\tMANAGED\tSYSTEM\tWILL USE")
			for _, t := range tools.All {
				s := r.Status(t)
				fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\n",
					t, s.Pinned,
					yesNo(s.ManagedPath != ""),
					orDash(s.SystemPath),
					orDash(s.WillUse),
				)
			}
			return w.Flush()
		},
	}
}

func newDepsInstallCmd(cfgFile *string, update bool) *cobra.Command {
	use, short := "install [tool...]", "Download missing managed tools (all, or named)"
	if update {
		use, short = "update [tool...]", "Re-download pinned managed tools, overwriting existing copies"
	}
	return &cobra.Command{
		Use:   use,
		Short: short,
		RunE: func(cmd *cobra.Command, args []string) error {
			selected, err := parseToolArgs(args)
			if err != nil {
				return err
			}
			r := depsResolver(cfgFile, true)
			for _, t := range selected {
				if !tools.SupportedPlatform(t) {
					fmt.Fprintf(os.Stderr, "  %s — no managed build for this platform, skipping\n", t)
					continue
				}
				path, err := r.Install(cmd.Context(), t, update)
				if err != nil {
					return fmt.Errorf("%s: %w", t, err)
				}
				fmt.Printf("%s %s ready: %s\n", t, tools.PinnedVersion(t), path)
			}
			return nil
		},
	}
}

func yesNo(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

func orDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}
