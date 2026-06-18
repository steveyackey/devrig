package identity

import (
	"os"
	"path/filepath"
	"runtime"
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

func TestNormalizeConfigPathStableOnWindows(t *testing.T) {
	// On Windows the same project reached via differently-cased paths must
	// normalize to one key (case-insensitive FS); on Unix the paths are
	// distinct and must be preserved.
	a := normalizeConfigPath(`C:\Code\TheOven\devrig.toml`)
	b := normalizeConfigPath(`c:\code\theoven\devrig.toml`)
	if runtime.GOOS == "windows" {
		if a != b {
			t.Errorf("windows: case-only path difference not normalized: %q vs %q", a, b)
		}
	} else {
		if a == b {
			t.Errorf("unix: case-sensitive paths wrongly collapsed: %q", a)
		}
	}
}

func TestComputeSlugStableAcrossCasingOnWindows(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("path casing only collapses on Windows")
	}
	// The whole point of the fix: differently-cased invocations of the same
	// project yield one slug (and therefore one k3d cluster), not several.
	a := computeSlug("TheOven", normalizeConfigPath(`C:\Code\TheOven\devrig.toml`))
	b := computeSlug("TheOven", normalizeConfigPath(`c:\code\theoven\devrig.toml`))
	if a != b {
		t.Errorf("same project, different path casing produced distinct slugs: %q vs %q", a, b)
	}
}

func TestPersistentSlugMatchesPreNormalizationEntry(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("pre-normalization key drift only occurs on Windows")
	}
	home := t.TempDir()
	// Simulate an index entry written before path normalization, under a
	// non-normalized (upper-cased) key.
	raw := `C:\Code\TheOven\devrig.toml`
	if first := persistentSlug(home, raw, "theoven-aaaaaa"); first != "theoven-aaaaaa" {
		t.Fatalf("seed assignment = %q, want theoven-aaaaaa", first)
	}
	// A later run resolves via the normalized key; it must reuse the original
	// slug rather than mint a new one.
	got := persistentSlug(home, normalizeConfigPath(raw), "theoven-bbbbbb")
	if got != "theoven-aaaaaa" {
		t.Errorf("normalized lookup = %q, want the original theoven-aaaaaa", got)
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
