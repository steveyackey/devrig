package cluster

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os/exec"
	"strings"
	"time"
)

// RegistryHostPort discovers the host-side port for the k3d registry container
// by inspecting its Docker port bindings.
func RegistryHostPort(containerName string) (uint16, error) {
	cmd := exec.Command("docker", "inspect", containerName,
		"--format", `{{json .NetworkSettings.Ports}}`)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return 0, fmt.Errorf("docker inspect %s: %w\n%s", containerName, err, out)
	}

	// Ports map: "5000/tcp": [{"HostIp":"0.0.0.0","HostPort":"XXXXX"}]
	var ports map[string][]struct {
		HostIP   string `json:"HostIp"`
		HostPort string `json:"HostPort"`
	}
	if err := json.Unmarshal([]byte(strings.TrimSpace(string(out))), &ports); err != nil {
		return 0, fmt.Errorf("parse docker port bindings: %w", err)
	}

	for _, bindings := range ports {
		for _, b := range bindings {
			if b.HostPort != "" {
				var port uint16
				if _, err := fmt.Sscanf(b.HostPort, "%d", &port); err == nil {
					return port, nil
				}
			}
		}
	}
	return 0, fmt.Errorf("no host port found for %s", containerName)
}

// WaitForRegistry polls the registry's /v2/ endpoint until it responds 200.
func WaitForRegistry(ctx context.Context, port uint16) error {
	url := fmt.Sprintf("http://localhost:%d/v2/", port)
	client := &http.Client{Timeout: 3 * time.Second}
	return pollWithBackoff(ctx, 15*time.Second, 250*time.Millisecond, 3*time.Second, func() error {
		resp, err := client.Get(url)
		if err != nil {
			return err
		}
		resp.Body.Close()
		if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusUnauthorized {
			return fmt.Errorf("registry returned %d", resp.StatusCode)
		}
		return nil
	})
}
