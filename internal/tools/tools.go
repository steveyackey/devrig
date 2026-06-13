// Package tools manages the external CLIs devrig's cluster features depend on
// (k3d, kubectl, helm). It can resolve them from the user's PATH or fetch
// pinned, checksum-verified copies into a devrig-private directory
// (~/.devrig/bin) so the cluster workflow works on a machine that has none of
// them installed — without shadowing the user's own copies on PATH.
//
// See docs/prd/managed-tool-deps.md for the design.
package tools

import (
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
)

// Tool identifies a managed external dependency.
type Tool string

const (
	Kubectl Tool = "kubectl"
	Helm    Tool = "helm"
	K3d     Tool = "k3d"
)

// All lists every managed tool, in a stable order.
var All = []Tool{Kubectl, Helm, K3d}

// PreferVendored and PreferSystem are the valid values for Options.Prefer.
const (
	PreferVendored = "vendored"
	PreferSystem   = "system"
)

// PinnedVersion returns the version devrig fetches and validates for a tool.
func PinnedVersion(t Tool) string { return pinnedVersion[t] }

// Options configures a Resolver.
type Options struct {
	// Prefer chooses managed ("vendored", the default) or "system" binaries
	// when both are available.
	Prefer string
	// Overrides maps a tool to an explicit executable path that wins over both
	// managed and system resolution.
	Overrides map[Tool]string
	// Dir is the managed-binary directory. Defaults to ~/.devrig/bin.
	Dir string
	// AllowFetch permits downloading a missing managed tool. Callers typically
	// set this from a TTY check; `devrig deps install` forces it true.
	AllowFetch bool
	// Stderr receives human-facing progress ("Fetching managed kubectl ...").
	// Defaults to os.Stderr.
	Stderr io.Writer
}

// Resolver resolves tools to absolute executable paths per its Options.
type Resolver struct {
	opts Options
}

// NewResolver returns a Resolver, filling in defaults for unset Options.
func NewResolver(o Options) *Resolver {
	if o.Prefer != PreferSystem {
		o.Prefer = PreferVendored
	}
	if o.Dir == "" {
		o.Dir = defaultDir()
	}
	if o.Stderr == nil {
		o.Stderr = os.Stderr
	}
	if o.Overrides == nil {
		o.Overrides = map[Tool]string{}
	}
	return &Resolver{opts: o}
}

// defaultDir is ~/.devrig/bin (falls back to ./.devrig/bin if $HOME is unset).
func defaultDir() string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		home = "."
	}
	return filepath.Join(home, ".devrig", "bin")
}

// Path resolves a tool to an absolute executable path, fetching a managed copy
// if necessary and permitted. Precedence depends on Options.Prefer; see the
// package doc and PRD.
func (r *Resolver) Path(ctx context.Context, t Tool) (string, error) {
	// 1. Explicit override always wins.
	if p := r.opts.Overrides[t]; p != "" {
		if _, err := os.Stat(p); err != nil {
			return "", fmt.Errorf("%s override %q: %w", t, p, err)
		}
		return p, nil
	}

	managed := func() (string, bool) {
		p := r.ManagedPath(t)
		if _, err := os.Stat(p); err == nil {
			return p, true
		}
		return "", false
	}
	system := func() (string, bool) {
		if p, err := exec.LookPath(string(t)); err == nil {
			return p, true
		}
		return "", false
	}

	// Ordered candidate lookups by preference.
	first, second := managed, system
	if r.opts.Prefer == PreferSystem {
		first, second = system, managed
	}
	if p, ok := first(); ok {
		return p, nil
	}

	// Neither preferred source had it yet. For vendored-preference, try to
	// fetch the managed copy before falling back to system.
	if r.opts.Prefer == PreferVendored && r.opts.AllowFetch {
		if p, err := r.fetch(ctx, t); err == nil {
			return p, nil
		} else if !isUnsupportedPlatform(err) {
			return "", err
		}
		// Unsupported platform: fall through to system/secondary.
	}

	if p, ok := second(); ok {
		return p, nil
	}

	// Last resort: fetch even when system was preferred but absent.
	if r.opts.AllowFetch {
		if p, err := r.fetch(ctx, t); err == nil {
			return p, nil
		}
	}

	return "", &NotFoundError{Tool: t, Dir: r.opts.Dir}
}

// ManagedPath returns the absolute path where the pinned managed copy of t
// lives (whether or not it currently exists).
func (r *Resolver) ManagedPath(t Tool) string {
	return filepath.Join(r.opts.Dir, managedName(t))
}

// Command builds an *exec.Cmd for a tool, resolving its path first.
func (r *Resolver) Command(ctx context.Context, t Tool, args ...string) (*exec.Cmd, error) {
	p, err := r.Path(ctx, t)
	if err != nil {
		return nil, err
	}
	return exec.CommandContext(ctx, p, args...), nil
}

// Status describes how a tool currently resolves, for `devrig deps`/`doctor`.
type Status struct {
	Tool         Tool
	Pinned       string
	ManagedPath  string // path if a managed pinned copy exists, else ""
	SystemPath   string // path if found on PATH, else ""
	OverridePath string // explicit override, else ""
	WillUse      string // the path Path() would return without fetching, else ""
}

// Status reports resolution state for a tool without fetching anything.
func (r *Resolver) Status(t Tool) Status {
	s := Status{Tool: t, Pinned: pinnedVersion[t]}
	if p := r.opts.Overrides[t]; p != "" {
		if _, err := os.Stat(p); err == nil {
			s.OverridePath = p
		}
	}
	if p := r.ManagedPath(t); fileExists(p) {
		s.ManagedPath = p
	}
	if p, err := exec.LookPath(string(t)); err == nil {
		s.SystemPath = p
	}

	switch {
	case s.OverridePath != "":
		s.WillUse = s.OverridePath
	case r.opts.Prefer == PreferSystem:
		s.WillUse = firstNonEmpty(s.SystemPath, s.ManagedPath)
	default:
		s.WillUse = firstNonEmpty(s.ManagedPath, s.SystemPath)
	}
	return s
}

// NotFoundError is returned when a tool cannot be resolved or fetched.
type NotFoundError struct {
	Tool Tool
	Dir  string
}

func (e *NotFoundError) Error() string {
	return fmt.Sprintf("%s not found on PATH and no managed copy in %s — run `devrig deps install %s` or install it yourself",
		e.Tool, e.Dir, e.Tool)
}

func fileExists(p string) bool {
	_, err := os.Stat(p)
	return err == nil
}

func firstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if v != "" {
			return v
		}
	}
	return ""
}
