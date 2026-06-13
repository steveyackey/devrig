// Package ports handles port allocation, sticky auto-ports, and conflict detection.
package ports

import (
	"fmt"
	"net"

	"github.com/steveyackey/devrig/internal/config"
)

// IsAvailable returns true if the given TCP port is free on localhost.
func IsAvailable(port uint16) bool {
	l, err := net.Listen("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		return false
	}
	_ = l.Close()
	return true
}

// FindFree finds an available ephemeral port by binding to :0 and reading the
// assigned port from the OS.
func FindFree() (uint16, error) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, fmt.Errorf("bind ephemeral port: %w", err)
	}
	port := uint16(l.Addr().(*net.TCPAddr).Port)
	_ = l.Close()
	return port, nil
}

// FindFreeExcluding finds a free port not in the allocated set.
func FindFreeExcluding(allocated map[uint16]bool) (uint16, error) {
	for range 100 {
		p, err := FindFree()
		if err != nil {
			return 0, err
		}
		if !allocated[p] {
			return p, nil
		}
	}
	return 0, fmt.Errorf("failed to find a free port after 100 attempts")
}

// Resolve returns a concrete port number for a config Port value. If the port
// is Auto, it tries to reuse prevPort (sticky auto-port) if it's still free,
// otherwise finds a fresh one. The chosen port is added to allocated.
func Resolve(portCfg *config.Port, label string, prevPort *uint16, prevAuto bool, allocated map[uint16]bool) (uint16, bool, error) {
	if portCfg == nil {
		// No port configured — still find one if auto needed.
		p, err := FindFreeExcluding(allocated)
		if err != nil {
			return 0, true, fmt.Errorf("%s: %w", label, err)
		}
		allocated[p] = true
		return p, true, nil
	}

	if !portCfg.IsAuto() {
		p := portCfg.AsFixed()
		allocated[p] = true
		return p, false, nil
	}

	// Auto: try sticky first.
	if prevAuto && prevPort != nil && !allocated[*prevPort] && IsAvailable(*prevPort) {
		allocated[*prevPort] = true
		return *prevPort, true, nil
	}

	p, err := FindFreeExcluding(allocated)
	if err != nil {
		return 0, true, fmt.Errorf("%s: %w", label, err)
	}
	allocated[p] = true
	return p, true, nil
}

// ResolveFixed allocates a fixed-or-auto port for a dashboard/OTel endpoint,
// falling back to a free port if the preferred one is taken.
func ResolveFixed(portCfg config.Port, label string, allocated map[uint16]bool) (uint16, error) {
	if portCfg.IsAuto() || portCfg.IsZero() {
		p, err := FindFreeExcluding(allocated)
		if err != nil {
			return 0, fmt.Errorf("%s: %w", label, err)
		}
		allocated[p] = true
		return p, nil
	}
	preferred := portCfg.AsFixed()
	if !allocated[preferred] && IsAvailable(preferred) {
		allocated[preferred] = true
		return preferred, nil
	}
	// Preferred is taken — find a free one.
	p, err := FindFreeExcluding(allocated)
	if err != nil {
		return 0, fmt.Errorf("%s: preferred port %d in use, fallback failed: %w", label, preferred, err)
	}
	allocated[p] = true
	return p, nil
}

// Conflict describes a port already in use before devrig starts.
type Conflict struct {
	Resource string
	Port     uint16
}

func (c Conflict) Error() string {
	return fmt.Sprintf("port %d required by %q is already in use", c.Port, c.Resource)
}

// CheckFixed verifies that all fixed (non-auto) ports in the config are free
// on the system right now. Returns one Conflict per violation.
func CheckFixed(cfg *config.Config) []Conflict {
	var out []Conflict

	check := func(port *config.Port, label string) {
		if port == nil || port.IsAuto() || port.IsZero() {
			return
		}
		if !IsAvailable(port.AsFixed()) {
			out = append(out, Conflict{Resource: label, Port: port.AsFixed()})
		}
	}
	checkVal := func(port config.Port, label string) {
		if port.IsAuto() || port.IsZero() {
			return
		}
		if !IsAvailable(port.AsFixed()) {
			out = append(out, Conflict{Resource: label, Port: port.AsFixed()})
		}
	}

	for name, svc := range cfg.Services {
		check(svc.Port, fmt.Sprintf("services.%s", name))
	}
	for name, d := range cfg.Docker {
		check(d.Port, fmt.Sprintf("docker.%s", name))
		for portName, p := range d.Ports {
			checkVal(p, fmt.Sprintf("docker.%s.ports.%s", name, portName))
		}
	}
	if cfg.Dashboard != nil {
		checkVal(cfg.Dashboard.Port, "dashboard")
		if cfg.Dashboard.OTel != nil {
			checkVal(cfg.Dashboard.OTel.GRPCPort, "dashboard.otel.grpc_port")
			checkVal(cfg.Dashboard.OTel.HTTPPort, "dashboard.otel.http_port")
		}
	}
	return out
}
