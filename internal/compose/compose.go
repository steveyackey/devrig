// Package compose wraps the docker-compose CLI for devrig managed services.
package compose

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"

	"github.com/steveyackey/devrig/internal/verbose"
)

// Service represents a running compose service from `docker compose ps --format json`.
type Service struct {
	ID         string      `json:"ID"`
	Name       string      `json:"Name"`
	Service    string      `json:"Service"`
	State      string      `json:"State"`
	Health     string      `json:"Health"`
	Publishers []Publisher `json:"Publishers"`
}

// Publisher is a port binding reported by docker compose ps.
type Publisher struct {
	TargetPort    uint16 `json:"TargetPort"`
	PublishedPort uint16 `json:"PublishedPort"`
}

// Up runs `docker compose up -d` for the named services.
func Up(composeFile, projectName string, services []string, envFile string) error {
	args := []string{"compose", "-f", composeFile, "-p", projectName, "up", "-d"}
	if envFile != "" {
		args = append(args, "--env-file", envFile)
	}
	args = append(args, services...)
	if out, err := verbose.Run(exec.Command("docker", args...)); err != nil {
		return fmt.Errorf("docker compose up: %w\n%s", err, out)
	}
	return nil
}

// Down runs `docker compose down --remove-orphans`.
func Down(composeFile, projectName string) error {
	cmd := exec.Command("docker", "compose", "-f", composeFile, "-p", projectName, "down", "--remove-orphans")
	if out, err := verbose.Run(cmd); err != nil {
		return fmt.Errorf("docker compose down: %w\n%s", err, out)
	}
	return nil
}

// PS runs `docker compose ps --format json` and parses the output.
func PS(composeFile, projectName string) ([]Service, error) {
	cmd := exec.Command("docker", "compose", "-f", composeFile, "-p", projectName, "ps", "--format", "json")
	trimmed, stderr, err := verbose.RunSplit(cmd)
	if err != nil {
		return nil, fmt.Errorf("docker compose ps: %w\n%s", err, stderr)
	}

	if trimmed == "" {
		return nil, nil
	}

	// Try JSON array first (newer docker compose versions).
	var services []Service
	if err := json.Unmarshal([]byte(trimmed), &services); err == nil {
		return services, nil
	}

	// Fall back to newline-delimited JSON.
	scanner := bufio.NewScanner(strings.NewReader(trimmed))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		var svc Service
		if err := json.Unmarshal([]byte(line), &svc); err != nil {
			return nil, fmt.Errorf("parsing compose ps output: %w", err)
		}
		services = append(services, svc)
	}
	return services, nil
}

// DiscoverServices reads a docker-compose.yml and returns the service names
// defined under the top-level `services:` key — without running Docker.
func DiscoverServices(composePath string) []string {
	data, err := readLines(composePath)
	if err != nil {
		return nil
	}

	var services []string
	inServices := false
	serviceIndent := -1

	for _, line := range data {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" || strings.HasPrefix(trimmed, "#") {
			continue
		}
		indent := len(line) - len(strings.TrimLeft(line, " \t"))

		if !inServices {
			if indent == 0 && strings.HasPrefix(trimmed, "services:") {
				inServices = true
			}
			continue
		}

		if indent == 0 {
			break // hit another top-level key
		}

		if serviceIndent < 0 {
			serviceIndent = indent
		}

		if indent == serviceIndent {
			if idx := strings.Index(trimmed, ":"); idx > 0 {
				name := trimmed[:idx]
				if name != "" {
					services = append(services, name)
				}
			}
		}
	}
	return services
}

func readLines(path string) ([]string, error) {
	data, err := exec.Command("cat", path).Output()
	if err != nil {
		return nil, err
	}
	return strings.Split(string(data), "\n"), nil
}
