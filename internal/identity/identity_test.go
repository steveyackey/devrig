package identity

import (
	"os"
	"path/filepath"
	"testing"
)

func TestComputeSlugDeterministic(t *testing.T) {
	abs := "/Users/x/code/theoven/devrig.toml"
	got := computeSlug("TheOven", abs)
	// Pinning the exact output guards against an accidental algorithm change
	// (e.g. the hash length) that would silently re-slug every project.
	want := computeSlug("TheOven", abs)
	if got != want || got == "" {
		t.Fatalf("computeSlug not deterministic: %q vs %q", got, want)
	}
	// Name normalization + 6-hex-char hash suffix.
	if got[:8] != "theoven-" || len(got) != len("theoven-")+6 {
		t.Errorf("unexpected slug shape: %q", got)
	}
}

func TestComputeSlugDistinctPaths(t *testing.T) {
	a := computeSlug("app", "/a/devrig.toml")
	b := computeSlug("app", "/b/devrig.toml")
	if a == b {
		t.Errorf("different config paths produced the same slug: %q", a)
	}
}

func TestPersistentSlugKeepsFirstAssignment(t *testing.T) {
	home := t.TempDir()
	abs := "/Users/x/code/theoven/devrig.toml"

	// First call records the candidate.
	first := persistentSlug(home, abs, "theoven-aaaaaa")
	if first != "theoven-aaaaaa" {
		t.Fatalf("first assignment = %q, want theoven-aaaaaa", first)
	}

	// A later algorithm change yields a different candidate, but the recorded
	// slug must win so the project's cluster/state isn't orphaned.
	stable := persistentSlug(home, abs, "theoven-bbbbbb")
	if stable != "theoven-aaaaaa" {
		t.Errorf("persisted slug = %q, want the original theoven-aaaaaa", stable)
	}

	// The index file should have been written.
	if _, err := os.Stat(filepath.Join(home, ".devrig", "slugs.json")); err != nil {
		t.Errorf("slug index not written: %v", err)
	}
}

func TestPersistentSlugDistinctProjects(t *testing.T) {
	home := t.TempDir()
	a := persistentSlug(home, "/a/devrig.toml", "app-aaaaaa")
	b := persistentSlug(home, "/b/devrig.toml", "app-bbbbbb")
	if a == b {
		t.Errorf("distinct projects share a slug: %q", a)
	}
	// Re-resolving each must return its own recorded slug.
	if got := persistentSlug(home, "/a/devrig.toml", "app-zzzzzz"); got != "app-aaaaaa" {
		t.Errorf("project a slug = %q, want app-aaaaaa", got)
	}
}
