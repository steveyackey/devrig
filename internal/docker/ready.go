package docker

import (
	"context"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"time"

	dockerclient "github.com/docker/docker/client"
	"github.com/docker/docker/api/types/container"

	"github.com/steveyackey/devrig/internal/config"
)

// RunReadyCheck polls until the container passes its ready check or the
// configured timeout expires.
func RunReadyCheck(
	ctx context.Context,
	cli *dockerclient.Client,
	containerID string,
	check *config.ReadyCheck,
	hostPort *uint16,
	name string,
) error {
	timeout := time.Duration(check.TimeoutSecs()) * time.Second
	if timeout == 0 {
		timeout = 60 * time.Second
	}

	if check.Type == "log" {
		return waitForLogPattern(ctx, cli, containerID, check.Match, timeout, name)
	}

	deadline := time.Now().Add(timeout)
	delay := 250 * time.Millisecond
	maxDelay := 3 * time.Second

	for {
		err := runSingleCheck(ctx, cli, containerID, check, hostPort)
		if err == nil {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("ready check for %q timed out after %s: %v", name, timeout, err)
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(delay):
		}
		delay = min(delay*2, maxDelay)
	}
}

func runSingleCheck(ctx context.Context, cli *dockerclient.Client, containerID string, check *config.ReadyCheck, hostPort *uint16) error {
	switch check.Type {
	case "pg_isready":
		code, _, err := execInContainer(ctx, cli, containerID, []string{"pg_isready", "-h", "localhost", "-q", "-t", "2"})
		if err != nil {
			return err
		}
		if code != 0 {
			return fmt.Errorf("pg_isready returned exit code %d", code)
		}
		return nil

	case "cmd":
		return ExecReadyCheck(ctx, cli, containerID, check.Command, check.Expect)

	case "http":
		httpClient := &http.Client{Timeout: 2 * time.Second}
		resp, err := httpClient.Get(check.URL)
		if err != nil {
			return err
		}
		_ = resp.Body.Close()
		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			return fmt.Errorf("HTTP ready check returned status %d", resp.StatusCode)
		}
		return nil

	case "tcp":
		if hostPort == nil {
			return fmt.Errorf("TCP ready check requires a port")
		}
		conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", *hostPort), 2*time.Second)
		if err != nil {
			return err
		}
		_ = conn.Close()
		return nil

	default:
		return fmt.Errorf("unknown ready check type %q", check.Type)
	}
}

func waitForLogPattern(ctx context.Context, cli *dockerclient.Client, containerID, pattern string, timeout time.Duration, name string) error {
	timeoutCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	rc, err := cli.ContainerLogs(timeoutCtx, containerID, container.LogsOptions{
		ShowStdout: true,
		ShowStderr: true,
		Follow:     true,
		Tail:       "all",
	})
	if err != nil {
		return fmt.Errorf("streaming logs for %s: %w", name, err)
	}
	defer rc.Close()

	buf := make([]byte, 4096)
	var accumulated strings.Builder
	for {
		n, err := rc.Read(buf)
		if n > 0 {
			accumulated.Write(buf[:n])
			if strings.Contains(accumulated.String(), pattern) {
				return nil
			}
		}
		if err != nil {
			if err == io.EOF {
				break
			}
			if timeoutCtx.Err() != nil {
				break
			}
		}
	}
	return fmt.Errorf("log ready check for %q timed out after %s (pattern: %q)", name, timeout, pattern)
}

func min(a, b time.Duration) time.Duration {
	if a < b {
		return a
	}
	return b
}
