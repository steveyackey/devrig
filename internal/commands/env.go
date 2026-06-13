package commands

import (
	"fmt"
	"sort"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/identity"
	"github.com/steveyackey/devrig/internal/state"
)

// NewEnvCmd prints the resolved environment variables for a named service.
func NewEnvCmd(cfgFile *string) *cobra.Command {
	return &cobra.Command{
		Use:   "env <service>",
		Short: "Print resolved DEVRIG_* environment for a service",
		Args:  cobra.ExactArgs(1),
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

			svcName := args[0]
			// Check service exists.
			if _, ok := cfg.Services[svcName]; !ok {
				return fmt.Errorf("service %q not found in config", svcName)
			}

			env := buildEnv(cfg, st)
			keys := make([]string, 0, len(env))
			for k := range env {
				keys = append(keys, k)
			}
			sort.Strings(keys)
			for _, k := range keys {
				fmt.Printf("%s=%s\n", k, env[k])
			}
			return nil
		},
	}
}

// buildEnv reconstructs the DEVRIG_* env map from current config + state.
func buildEnv(cfg *config.Config, st *state.ProjectState) map[string]string {
	env := make(map[string]string)
	for name, svcCfg := range cfg.Services {
		upper := toUpper(name)
		env[fmt.Sprintf("DEVRIG_%s_HOST", upper)] = "localhost"
		if st != nil {
			if ss, ok := st.Services[name]; ok && ss.Port != nil {
				env[fmt.Sprintf("DEVRIG_%s_PORT", upper)] = fmt.Sprint(*ss.Port)
				env[fmt.Sprintf("DEVRIG_%s_URL", upper)] = fmt.Sprintf("http://localhost:%d", *ss.Port)
			}
		}
		_ = svcCfg
	}
	if st != nil && st.Dashboard != nil {
		env["DEVRIG_DASHBOARD_URL"] = fmt.Sprintf("http://localhost:%d", st.Dashboard.DashboardPort)
		env["OTEL_EXPORTER_OTLP_ENDPOINT"] = fmt.Sprintf("http://localhost:%d", st.Dashboard.HTTPPort)
		env["OTEL_EXPORTER_OTLP_GRPC_ENDPOINT"] = fmt.Sprintf("http://localhost:%d", st.Dashboard.GRPCPort)
	}
	return env
}

func toUpper(s string) string {
	upper := make([]byte, len(s))
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c >= 'a' && c <= 'z' {
			c -= 32
		} else if c == '-' || c == '.' {
			c = '_'
		}
		upper[i] = c
	}
	return string(upper)
}
