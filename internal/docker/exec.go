package docker

import (
	"context"
	"fmt"
	"io"
	"strings"

	dockerclient "github.com/docker/docker/client"
	"github.com/docker/docker/api/types/container"
)

// execInContainer runs a command in a container and returns (exitCode, output, error).
func execInContainer(ctx context.Context, cli *dockerclient.Client, containerID string, cmd []string) (int, string, error) {
	exec, err := cli.ContainerExecCreate(ctx, containerID, container.ExecOptions{
		Cmd:          cmd,
		AttachStdout: true,
		AttachStderr: true,
	})
	if err != nil {
		return -1, "", fmt.Errorf("creating exec: %w", err)
	}

	resp, err := cli.ContainerExecAttach(ctx, exec.ID, container.ExecAttachOptions{})
	if err != nil {
		return -1, "", fmt.Errorf("attaching exec: %w", err)
	}
	defer resp.Close()

	var sb strings.Builder
	_, _ = io.Copy(&sb, resp.Reader)

	inspect, err := cli.ContainerExecInspect(ctx, exec.ID)
	if err != nil {
		return -1, sb.String(), fmt.Errorf("inspecting exec: %w", err)
	}

	return inspect.ExitCode, sb.String(), nil
}

// ExecReadyCheck runs a command-based ready check via docker exec.
func ExecReadyCheck(ctx context.Context, cli *dockerclient.Client, containerID, command string, expect *string) error {
	code, out, err := execInContainer(ctx, cli, containerID, []string{"sh", "-c", command})
	if err != nil {
		return err
	}
	if code != 0 {
		return fmt.Errorf("command %q exited with code %d", command, code)
	}
	if expect != nil && !strings.Contains(out, *expect) {
		return fmt.Errorf("command %q output did not contain %q (got: %q)", command, *expect, strings.TrimSpace(out))
	}
	return nil
}
