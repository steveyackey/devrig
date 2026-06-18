// Command devrig is the local development orchestrator.
//
//	devrig start              # start all services
//	devrig stop               # stop all services
//	devrig ps                 # show service status
//	devrig validate           # validate the config file
//	devrig init               # generate a starter devrig.toml
//	devrig doctor             # check dependencies are installed
//	devrig env <service>      # print resolved env for a service
//	devrig exec <service>     # exec into a container
//	devrig reset              # clear init_completed flags
//	devrig logs               # query stored logs
//	devrig query              # query OTel telemetry
//	devrig cluster            # manage k3d cluster
//	devrig update             # update the devrig binary
//	devrig completion         # generate shell completion script
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
	"github.com/steveyackey/devrig/internal/commands"
	"github.com/steveyackey/devrig/internal/verbose"
)

var version = "dev"

func main() {
	startStartupProfiling()

	var cfgFile string
	var verboseFlag bool

	root := &cobra.Command{
		Use:     "devrig",
		Short:   "Local development orchestrator",
		Version: version,
		// Honor --verbose before any subcommand runs. (DEVRIG_VERBOSE=1 in the
		// environment enables it too, without the flag.)
		PersistentPreRun: func(_ *cobra.Command, _ []string) {
			if verboseFlag {
				verbose.Enable()
			}
		},
	}

	root.PersistentFlags().StringVarP(&cfgFile, "file", "f", "", "Config file (default: walk up for devrig.toml)")
	root.PersistentFlags().BoolVarP(&verboseFlag, "verbose", "v", false, "Stream all tool/subprocess output live (also via DEVRIG_VERBOSE=1)")

	root.AddCommand(
		commands.NewValidateCmd(&cfgFile),
		commands.NewInitCmd(),
		commands.NewStartCmd(&cfgFile),
		commands.NewStopCmd(&cfgFile),
		commands.NewDeleteCmd(&cfgFile),
		commands.NewPsCmd(),
		commands.NewDoctorCmd(&cfgFile),
		commands.NewDepsCmd(&cfgFile),
		commands.NewEnvCmd(&cfgFile),
		commands.NewExecCmd(&cfgFile),
		commands.NewResetCmd(&cfgFile),
		commands.NewLogsCmd(&cfgFile),
		commands.NewQueryCmd(&cfgFile),
		commands.NewClusterCmd(&cfgFile),
		commands.NewKubectlCmd(&cfgFile),
		commands.NewSkillCmd(&cfgFile),
		commands.NewUpdateCmd(version),
	)
	// cobra registers a hidden `__complete` command for shell completion.
	// Expose the standard `completion` subcommand for user-facing use.
	root.AddCommand(completionCmd(root))

	if err := root.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func completionCmd(root *cobra.Command) *cobra.Command {
	cmd := &cobra.Command{
		Use:   "completion [bash|zsh|fish|powershell]",
		Short: "Generate shell completion script",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			switch args[0] {
			case "bash":
				return root.GenBashCompletion(os.Stdout)
			case "zsh":
				return root.GenZshCompletion(os.Stdout)
			case "fish":
				return root.GenFishCompletion(os.Stdout, true)
			case "powershell":
				return root.GenPowerShellCompletionWithDesc(os.Stdout)
			default:
				return fmt.Errorf("unknown shell %q (bash|zsh|fish|powershell)", args[0])
			}
		},
	}
	return cmd
}
