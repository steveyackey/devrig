// Package docker wraps the Docker API client for devrig container lifecycle management.
package docker

import (
	"context"
	"fmt"
	"io"
	"path/filepath"
	"strings"
	"time"

	dockerclient "github.com/docker/docker/client"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/image"
	"github.com/docker/docker/api/types/network"
	"github.com/docker/docker/api/types/volume"
	"github.com/docker/go-connections/nat"

	"github.com/steveyackey/devrig/internal/config"
	"github.com/steveyackey/devrig/internal/ports"
	"github.com/steveyackey/devrig/internal/state"
)

// Manager manages Docker containers and networks for a devrig project.
type Manager struct {
	client *dockerclient.Client
	slug   string
}

// New creates a Manager and verifies Docker daemon connectivity.
func New(slug string) (*Manager, error) {
	cli, err := dockerclient.NewClientWithOpts(
		dockerclient.FromEnv,
		dockerclient.WithAPIVersionNegotiation(),
	)
	if err != nil {
		return nil, fmt.Errorf("creating Docker client: %w", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if _, err := cli.Ping(ctx); err != nil {
		return nil, fmt.Errorf("cannot connect to Docker daemon: %w", err)
	}
	return &Manager{client: cli, slug: slug}, nil
}

func (m *Manager) Client() *dockerclient.Client { return m.client }
func (m *Manager) Slug() string                  { return m.slug }
func (m *Manager) NetworkName() string            { return "devrig-" + m.slug + "-net" }

// resourceLabels returns the standard devrig labels for a resource.
func resourceLabels(slug, service string) map[string]string {
	return map[string]string{
		"devrig.project":    slug,
		"devrig.service":    service,
		"devrig.managed-by": "devrig",
	}
}

// EnsureNetwork creates the project bridge network if it doesn't already exist.
func (m *Manager) EnsureNetwork(ctx context.Context) error {
	name := m.NetworkName()
	_, err := m.client.NetworkInspect(ctx, name, network.InspectOptions{})
	if err == nil {
		return nil
	}
	if !dockerclient.IsErrNotFound(err) {
		return fmt.Errorf("inspect network %s: %w", name, err)
	}
	_, err = m.client.NetworkCreate(ctx, name, network.CreateOptions{
		Driver: "bridge",
		Labels: resourceLabels(m.slug, "network"),
	})
	return err
}

// RemoveNetwork removes the project network (ignores not-found).
func (m *Manager) RemoveNetwork(ctx context.Context) error {
	err := m.client.NetworkRemove(ctx, m.NetworkName())
	if dockerclient.IsErrNotFound(err) {
		return nil
	}
	return err
}

// ConnectContainerToNetwork connects a container to the project network with aliases.
func (m *Manager) ConnectContainerToNetwork(ctx context.Context, containerID string, aliases []string) error {
	return m.client.NetworkConnect(ctx, m.NetworkName(), containerID, &network.EndpointSettings{
		Aliases: aliases,
	})
}

// StartService starts a single docker service container.
func (m *Manager) StartService(
	ctx context.Context,
	name string,
	cfg *config.DockerConfig,
	prevState *state.DockerState,
	allocated map[uint16]bool,
	configDir string,
) (state.DockerState, error) {
	// Pull image if not present.
	if !imageExists(ctx, m.client, cfg.Image) {
		if err := pullImage(ctx, m.client, cfg.Image, cfg.RegistryAuth); err != nil {
			return state.DockerState{}, fmt.Errorf("pulling image %s: %w", cfg.Image, err)
		}
	}

	// Resolve ports.
	var hostPort *uint16
	portAuto := false
	namedPorts := make(map[string]uint16)

	if cfg.Port != nil {
		var prevPort *uint16
		prevAuto := false
		if prevState != nil {
			prevPort = prevState.Port
			prevAuto = prevState.PortAuto
		}
		p, auto, err := ports.Resolve(cfg.Port, fmt.Sprintf("docker:%s", name), prevPort, prevAuto, allocated)
		if err != nil {
			return state.DockerState{}, err
		}
		hostPort = &p
		portAuto = auto
	}
	for portName, portCfg := range cfg.Ports {
		var prevPort *uint16
		if prevState != nil {
			if p, ok := prevState.NamedPorts[portName]; ok {
				pp := p
				prevPort = &pp
			}
		}
		p, _, err := ports.Resolve(&portCfg, fmt.Sprintf("docker:%s:%s", name, portName), prevPort, portCfg.IsAuto(), allocated)
		if err != nil {
			return state.DockerState{}, err
		}
		namedPorts[portName] = p
	}

	// Create/ensure volumes.
	var binds []string
	for _, volSpec := range cfg.Volumes {
		source, containerPath, ok := parseVolumeSpec(volSpec)
		if !ok {
			continue
		}
		if isBindMount(source) {
			abs := source
			if !filepath.IsAbs(abs) {
				abs = filepath.Join(configDir, source)
			}
			binds = append(binds, abs+":"+containerPath)
		} else {
			volName := "devrig-" + m.slug + "-" + source
			if err := ensureVolume(ctx, m.client, volName, resourceLabels(m.slug, name)); err != nil {
				return state.DockerState{}, fmt.Errorf("ensuring volume %s: %w", volName, err)
			}
			binds = append(binds, volName+":"+containerPath)
		}
	}

	// Build port bindings.
	portBindings := nat.PortMap{}
	exposedPorts := nat.PortSet{}

	if hostPort != nil {
		var containerPort uint16
		if cfg.ContainerPort != nil {
			containerPort = *cfg.ContainerPort
		} else if cfg.Port != nil && !cfg.Port.IsAuto() {
			containerPort = cfg.Port.AsFixed()
		} else {
			containerPort = *hostPort
		}
		p := nat.Port(fmt.Sprintf("%d/tcp", containerPort))
		portBindings[p] = []nat.PortBinding{{HostIP: "0.0.0.0", HostPort: fmt.Sprint(*hostPort)}}
		exposedPorts[p] = struct{}{}
	}
	for portName, portCfg := range cfg.Ports {
		hp := namedPorts[portName]
		var cp uint16
		if !portCfg.IsAuto() {
			cp = portCfg.AsFixed()
		} else {
			cp = hp
		}
		p := nat.Port(fmt.Sprintf("%d/tcp", cp))
		portBindings[p] = []nat.PortBinding{{HostIP: "0.0.0.0", HostPort: fmt.Sprint(hp)}}
		exposedPorts[p] = struct{}{}
	}

	// Env vars.
	env := make([]string, 0, len(cfg.Env))
	for k, v := range cfg.Env {
		env = append(env, k+"="+v)
	}

	containerName := "devrig-" + m.slug + "-" + name
	labels := resourceLabels(m.slug, name)

	// Remove existing container (idempotent).
	_ = stopContainer(ctx, m.client, containerName, 5)
	_ = removeContainer(ctx, m.client, containerName, true)

	// Create container.
	containerCfg := &container.Config{
		Image:        cfg.Image,
		Env:          env,
		Labels:       labels,
		ExposedPorts: exposedPorts,
	}
	if cfg.Command != nil && len(*cfg.Command) > 0 {
		containerCfg.Cmd = []string(*cfg.Command)
	}
	if cfg.Entrypoint != nil && len(*cfg.Entrypoint) > 0 {
		containerCfg.Entrypoint = []string(*cfg.Entrypoint)
	}

	hostCfg := &container.HostConfig{
		PortBindings: portBindings,
		Binds:        binds,
		NetworkMode:  container.NetworkMode(m.NetworkName()),
	}

	resp, err := m.client.ContainerCreate(ctx, containerCfg, hostCfg, nil, nil, containerName)
	if err != nil {
		return state.DockerState{}, fmt.Errorf("creating container %s: %w", containerName, err)
	}
	containerID := resp.ID

	if err := m.client.ContainerStart(ctx, containerID, container.StartOptions{}); err != nil {
		return state.DockerState{}, fmt.Errorf("starting container %s: %w", containerName, err)
	}

	// Ready check.
	if cfg.ReadyCheck != nil {
		if err := RunReadyCheck(ctx, m.client, containerID, cfg.ReadyCheck, hostPort, name); err != nil {
			return state.DockerState{}, fmt.Errorf("ready check for %s: %w", name, err)
		}
	}

	// Init scripts (skip if already completed).
	alreadyInit := prevState != nil && prevState.InitCompleted
	var initCompletedAt *time.Time
	initCompleted := alreadyInit
	if prevState != nil {
		initCompletedAt = prevState.InitCompletedAt
	}

	if !alreadyInit && len(cfg.Init) > 0 {
		if err := runInitScripts(ctx, m.client, containerID, name, cfg); err != nil {
			return state.DockerState{}, fmt.Errorf("init scripts for %s: %w", name, err)
		}
		initCompleted = true
		now := time.Now()
		initCompletedAt = &now
	}

	return state.DockerState{
		ContainerID:     containerID,
		ContainerName:   containerName,
		Port:            hostPort,
		PortAuto:        portAuto,
		NamedPorts:      namedPorts,
		InitCompleted:   initCompleted,
		InitCompletedAt: initCompletedAt,
	}, nil
}

// StopService stops a container (does not remove it).
func (m *Manager) StopService(ctx context.Context, st *state.DockerState) error {
	return stopContainer(ctx, m.client, st.ContainerID, 10)
}

// DeleteService stops and removes a container.
func (m *Manager) DeleteService(ctx context.Context, st *state.DockerState) error {
	_ = stopContainer(ctx, m.client, st.ContainerID, 10)
	return removeContainer(ctx, m.client, st.ContainerID, true)
}

// CleanupAll removes all containers, volumes, and the network for this project.
func (m *Manager) CleanupAll(ctx context.Context) error {
	// Containers by label.
	f := filters.NewArgs(filters.Arg("label", "devrig.project="+m.slug))
	containers, err := m.client.ContainerList(ctx, container.ListOptions{All: true, Filters: f})
	if err != nil {
		return fmt.Errorf("listing containers: %w", err)
	}
	for _, c := range containers {
		_ = stopContainer(ctx, m.client, c.ID, 5)
		_ = removeContainer(ctx, m.client, c.ID, true)
	}

	// Volumes by label.
	vols, err := m.client.VolumeList(ctx, volume.ListOptions{Filters: f})
	if err == nil {
		for _, v := range vols.Volumes {
			_ = m.client.VolumeRemove(ctx, v.Name, false)
		}
	}

	_ = m.RemoveNetwork(ctx)
	return nil
}

// --- low-level helpers ---

func imageExists(ctx context.Context, cli *dockerclient.Client, ref string) bool {
	_, _, err := cli.ImageInspectWithRaw(ctx, ref)
	return err == nil
}

func pullImage(ctx context.Context, cli *dockerclient.Client, ref string, _ interface{}) error {
	rc, err := cli.ImagePull(ctx, ref, image.PullOptions{})
	if err != nil {
		return err
	}
	defer rc.Close()
	_, _ = io.Copy(io.Discard, rc) // drain to completion
	return nil
}

func stopContainer(ctx context.Context, cli *dockerclient.Client, id string, timeoutSecs int) error {
	timeout := timeoutSecs
	err := cli.ContainerStop(ctx, id, container.StopOptions{Timeout: &timeout})
	if dockerclient.IsErrNotFound(err) {
		return nil
	}
	return err
}

func removeContainer(ctx context.Context, cli *dockerclient.Client, id string, force bool) error {
	err := cli.ContainerRemove(ctx, id, container.RemoveOptions{Force: force})
	if dockerclient.IsErrNotFound(err) {
		return nil
	}
	return err
}

func ensureVolume(ctx context.Context, cli *dockerclient.Client, name string, labels map[string]string) error {
	_, err := cli.VolumeInspect(ctx, name)
	if err == nil {
		return nil
	}
	if !dockerclient.IsErrNotFound(err) {
		return err
	}
	_, err = cli.VolumeCreate(ctx, volume.CreateOptions{Name: name, Labels: labels})
	return err
}

func runInitScripts(ctx context.Context, cli *dockerclient.Client, containerID, dockerName string, cfg *config.DockerConfig) error {
	for i, script := range cfg.Init {
		var cmd []string
		if strings.Contains(cfg.Image, "postgres") {
			user := "postgres"
			if u, ok := cfg.Env["POSTGRES_USER"]; ok {
				user = u
			}
			cmd = []string{"psql", "-U", user, "-c", script}
		} else {
			cmd = []string{"sh", "-c", script}
		}
		code, out, err := execInContainer(ctx, cli, containerID, cmd)
		if err != nil {
			return fmt.Errorf("init script %d/%d for %s: %w", i+1, len(cfg.Init), dockerName, err)
		}
		if code != 0 {
			return fmt.Errorf("init script %d/%d for %s failed (exit %d): %s", i+1, len(cfg.Init), dockerName, code, out)
		}
	}
	return nil
}

func parseVolumeSpec(spec string) (source, containerPath string, ok bool) {
	idx := strings.Index(spec, ":")
	if idx < 0 {
		return "", "", false
	}
	s := spec[:idx]
	p := spec[idx+1:]
	if s == "" || p == "" {
		return "", "", false
	}
	return s, p, true
}

func isBindMount(source string) bool {
	return strings.HasPrefix(source, "/") ||
		strings.HasPrefix(source, "./") ||
		strings.HasPrefix(source, "../")
}
