package docker

import (
	"bufio"
	"context"

	dockerclient "github.com/docker/docker/client"
	"github.com/docker/docker/api/types/container"

	"github.com/steveyackey/devrig/internal/events"
)

// StreamContainerLogs subscribes to container logs and broadcasts each line
// as a KindLogRecord event until ctx is cancelled or the stream ends.
func StreamContainerLogs(ctx context.Context, cli *dockerclient.Client, containerID, serviceName string, b *events.Broadcaster) {
	rc, err := cli.ContainerLogs(ctx, containerID, container.LogsOptions{
		ShowStdout: true,
		ShowStderr: true,
		Follow:     true,
		Tail:       "0",
	})
	if err != nil {
		return
	}
	defer rc.Close()

	scanner := bufio.NewScanner(rc)
	for scanner.Scan() {
		line := scanner.Text()
		b.Send(events.TelemetryEvent{
			Kind:        events.KindLogRecord,
			Service:     serviceName,
			LogBody:     line,
			LogSeverity: events.DetectLogLevel(line),
		})
	}
}
