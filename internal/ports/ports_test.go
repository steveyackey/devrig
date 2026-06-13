package ports

import (
	"net"
	"testing"

	"github.com/steveyackey/devrig/internal/config"
)

func TestResolveFixedPort(t *testing.T) {
	p := config.FixedPort(33333)
	allocated := map[uint16]bool{}
	got, auto, err := Resolve(&p, "svc", nil, false, allocated)
	if err != nil {
		t.Fatal(err)
	}
	if got != 33333 || auto {
		t.Errorf("got %d auto=%v", got, auto)
	}
	if !allocated[33333] {
		t.Error("port not marked allocated")
	}
}

func TestResolveAutoPort(t *testing.T) {
	p := config.AutoPort()
	allocated := map[uint16]bool{}
	got, auto, err := Resolve(&p, "svc", nil, false, allocated)
	if err != nil {
		t.Fatal(err)
	}
	if got == 0 || !auto {
		t.Errorf("got %d auto=%v", got, auto)
	}
}

func TestResolveStickyAutoPort(t *testing.T) {
	// Find a free port to use as the "previous" sticky port.
	prev, err := FindFree()
	if err != nil {
		t.Fatal(err)
	}
	p := config.AutoPort()
	allocated := map[uint16]bool{}
	got, _, err := Resolve(&p, "svc", &prev, true, allocated)
	if err != nil {
		t.Fatal(err)
	}
	if got != prev {
		t.Errorf("sticky port not reused: got %d, want %d", got, prev)
	}
}

func TestResolveStickySkipsBusyPort(t *testing.T) {
	// Occupy a port so the sticky path must fall through.
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	busy := uint16(l.Addr().(*net.TCPAddr).Port)

	p := config.AutoPort()
	got, _, err := Resolve(&p, "svc", &busy, true, map[uint16]bool{})
	if err != nil {
		t.Fatal(err)
	}
	if got == busy {
		t.Errorf("reused busy port %d", busy)
	}
}

func TestResolveFixedFallsBack(t *testing.T) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	busy := uint16(l.Addr().(*net.TCPAddr).Port)

	got, err := ResolveFixed(config.FixedPort(busy), "dashboard", map[uint16]bool{})
	if err != nil {
		t.Fatal(err)
	}
	if got == busy {
		t.Errorf("ResolveFixed returned busy port %d", busy)
	}
}
