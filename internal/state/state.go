// Package state manages persistent devrig project state in .devrig/state.json.
package state

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// ProjectState is serialised to .devrig/state.json.
type ProjectState struct {
	Slug            string                         `json:"slug"`
	ConfigPath      string                         `json:"config_path"`
	StartedAt       time.Time                      `json:"started_at"`
	Services        map[string]ServiceState        `json:"services"`
	Docker          map[string]DockerState         `json:"docker"`
	ComposeServices map[string]ComposeServiceState `json:"compose_services"`
	NetworkName     *string                        `json:"network_name,omitempty"`
	Cluster         *ClusterState                  `json:"cluster,omitempty"`
	Dashboard       *DashboardState                `json:"dashboard,omitempty"`
	// PID of the main devrig process (written so `devrig stop` can signal it).
	PID int `json:"pid,omitempty"`
	// PIDStartTimeMs is the devrig process's creation time (ms since epoch),
	// stored so `devrig stop` can confirm the PID hasn't been reused before
	// signalling it.
	PIDStartTimeMs int64 `json:"pid_start_time_ms,omitempty"`
}

type ServiceState struct {
	PID uint32 `json:"pid"`
	// StartTimeMs is the service process's creation time (ms since epoch),
	// recorded alongside PID so a later run can reuse-proof identify and reap an
	// orphan left by a crashed devrig.
	StartTimeMs int64   `json:"start_time_ms,omitempty"`
	Port        *uint16 `json:"port,omitempty"`
	PortAuto    bool    `json:"port_auto"`
	Protocol    *string `json:"protocol,omitempty"`
	Phase       *string `json:"phase,omitempty"`
	ExitCode    *int    `json:"exit_code,omitempty"`
}

type DockerState struct {
	ContainerID     string            `json:"container_id"`
	ContainerName   string            `json:"container_name"`
	Port            *uint16           `json:"port,omitempty"`
	PortAuto        bool              `json:"port_auto"`
	Protocol        *string           `json:"protocol,omitempty"`
	NamedPorts      map[string]uint16 `json:"named_ports"`
	InitCompleted   bool              `json:"init_completed"`
	InitCompletedAt *time.Time        `json:"init_completed_at,omitempty"`
}

type ComposeServiceState struct {
	ContainerID   string  `json:"container_id"`
	ContainerName string  `json:"container_name"`
	Port          *uint16 `json:"port,omitempty"`
}

type ClusterState struct {
	ClusterName      string                        `json:"cluster_name"`
	KubeconfigPath   string                        `json:"kubeconfig_path"`
	RegistryName     *string                       `json:"registry_name,omitempty"`
	RegistryPort     *uint16                       `json:"registry_port,omitempty"`
	DeployedServices map[string]ClusterDeployState `json:"deployed_services"`
	InstalledAddons  map[string]AddonState         `json:"installed_addons"`
	// K3dVersion is the version of the k3d binary that created/last started the
	// cluster, used to detect version skew on reuse (see cluster.Manager.Ensure).
	K3dVersion string `json:"k3d_version,omitempty"`
}

type ClusterDeployState struct {
	ImageTag     string    `json:"image_tag"`
	LastDeployed time.Time `json:"last_deployed"`
}

type AddonState struct {
	AddonType   string    `json:"addon_type"`
	Namespace   string    `json:"namespace"`
	InstalledAt time.Time `json:"installed_at"`
}

type DashboardState struct {
	DashboardPort uint16 `json:"dashboard_port"`
	GRPCPort      uint16 `json:"grpc_port"`
	HTTPPort      uint16 `json:"http_port"`
}

// StateDir returns the .devrig directory for a project (next to devrig.toml).
func StateDir(configPath string) string {
	return filepath.Join(filepath.Dir(configPath), ".devrig")
}

// Save writes state to state.json atomically (tmp → rename).
func (s *ProjectState) Save(stateDir string) error {
	if err := os.MkdirAll(stateDir, 0o755); err != nil {
		return fmt.Errorf("create state dir: %w", err)
	}
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal state: %w", err)
	}
	tmp := filepath.Join(stateDir, "state.json.tmp")
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return fmt.Errorf("write state tmp: %w", err)
	}
	return os.Rename(tmp, filepath.Join(stateDir, "state.json"))
}

// Load reads state.json from stateDir. Returns nil if it doesn't exist or
// can't be parsed.
func Load(stateDir string) *ProjectState {
	data, err := os.ReadFile(filepath.Join(stateDir, "state.json"))
	if err != nil {
		return nil
	}
	var s ProjectState
	if err := json.Unmarshal(data, &s); err != nil {
		return nil
	}
	return &s
}

// Remove deletes state.json and attempts to remove the state directory if empty.
func Remove(stateDir string) error {
	path := filepath.Join(stateDir, "state.json")
	if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
		return err
	}
	_ = os.Remove(stateDir)
	return nil
}

// mu protects atomic file-level updates below.
var mu sync.Mutex

// UpdateServiceProc writes the PID and process start time for a service into
// state.json using an exclusive file lock.
func UpdateServiceProc(stateDir, service string, pid uint32, startTimeMs int64) {
	withLock(stateDir, func(s *ProjectState) {
		if s.Services == nil {
			s.Services = make(map[string]ServiceState)
		}
		st := s.Services[service]
		st.PID = pid
		st.StartTimeMs = startTimeMs
		s.Services[service] = st
	})
}

// UpdateServicePhase writes the phase field for a service.
func UpdateServicePhase(stateDir, service, phase string) {
	withLock(stateDir, func(s *ProjectState) {
		if s.Services == nil {
			s.Services = make(map[string]ServiceState)
		}
		st := s.Services[service]
		st.Phase = &phase
		s.Services[service] = st
	})
}

// UpdateServiceExit writes the exit code for a service.
func UpdateServiceExit(stateDir, service string, code int) {
	withLock(stateDir, func(s *ProjectState) {
		if s.Services == nil {
			s.Services = make(map[string]ServiceState)
		}
		st := s.Services[service]
		st.ExitCode = &code
		s.Services[service] = st
	})
}

// withLock loads state.json, applies fn, then saves — holding a file lock and
// the in-process mutex for the duration.
func withLock(stateDir string, fn func(*ProjectState)) {
	mu.Lock()
	defer mu.Unlock()

	path := filepath.Join(stateDir, "state.json")
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o644)
	if err != nil {
		return
	}
	defer f.Close()

	// Exclusive advisory lock across processes (multiple devrig instances).
	lockFile(f)
	defer unlockFile(f)

	var s ProjectState
	dec := json.NewDecoder(f)
	if err := dec.Decode(&s); err != nil {
		// File may be empty on first write — use zero value.
		s = ProjectState{}
	}

	fn(&s)

	data, err := json.MarshalIndent(&s, "", "  ")
	if err != nil {
		return
	}
	_ = f.Truncate(0)
	_, _ = f.Seek(0, 0)
	_, _ = f.Write(data)
}
