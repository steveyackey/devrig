package cluster

import (
	"path/filepath"
	"runtime"
	"testing"

	"github.com/steveyackey/devrig/internal/config"
)

func TestImageBuildOrder(t *testing.T) {
	// runtime's build_args reference base's tag — base must build first even
	// though "agent" sorts before "agent-base" alphabetically.
	images := map[string]config.ClusterImageConfig{
		"agent": {
			Context:   "agents/runtime",
			BuildArgs: map[string]string{"BASE_IMAGE": "{{ cluster.image.agent-base.tag }}"},
		},
		"agent-base": {Context: "agents"},
	}
	order, err := ImageBuildOrder(images)
	if err != nil {
		t.Fatalf("ImageBuildOrder: %v", err)
	}
	if len(order) != 2 || order[0] != "agent-base" || order[1] != "agent" {
		t.Fatalf("got order %v, want [agent-base agent]", order)
	}
}

func TestImageBuildOrderDependsOn(t *testing.T) {
	images := map[string]config.ClusterImageConfig{
		"a": {Context: "a", DependsOn: []string{"b"}},
		"b": {Context: "b"},
	}
	order, err := ImageBuildOrder(images)
	if err != nil {
		t.Fatalf("ImageBuildOrder: %v", err)
	}
	if order[0] != "b" || order[1] != "a" {
		t.Fatalf("got order %v, want [b a]", order)
	}
}

func TestImageBuildOrderCycle(t *testing.T) {
	images := map[string]config.ClusterImageConfig{
		"a": {Context: "a", DependsOn: []string{"b"}},
		"b": {Context: "b", DependsOn: []string{"a"}},
	}
	if _, err := ImageBuildOrder(images); err == nil {
		t.Fatal("expected cycle error")
	}
}

func TestResolveClusterVolume(t *testing.T) {
	configDir := filepath.Join(string(filepath.Separator), "home", "user", "proj")

	tests := []struct {
		name string
		spec string
		want string
	}{
		{
			name: "relative parent with nodefilter",
			spec: "../:/workspace@server:*",
			want: filepath.Join(configDir, "..") + ":/workspace@server:*",
		},
		{
			name: "relative dot without nodefilter",
			spec: "./data:/data",
			want: filepath.Join(configDir, "data") + ":/data",
		},
		{
			name: "relative subdir",
			spec: "sub/dir:/mnt@agent:*",
			want: filepath.Join(configDir, "sub/dir") + ":/mnt@agent:*",
		},
		{
			name: "named volume left unchanged",
			spec: "pgdata:/var/lib/postgresql/data",
			want: "pgdata:/var/lib/postgresql/data",
		},
		{
			name: "no dest separator left unchanged",
			spec: "somevolume",
			want: "somevolume",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := resolveClusterVolume(tt.spec, configDir); got != tt.want {
				t.Errorf("resolveClusterVolume(%q) = %q, want %q", tt.spec, got, tt.want)
			}
		})
	}
}

func TestResolveClusterVolumeAbsoluteUnchanged(t *testing.T) {
	configDir := filepath.Join(string(filepath.Separator), "home", "user", "proj")

	// An already-absolute POSIX host path must pass through untouched.
	posixAbs := "/abs/host:/workspace@server:*"
	if runtime.GOOS != "windows" {
		if got := resolveClusterVolume(posixAbs, configDir); got != posixAbs {
			t.Errorf("absolute path mangled: got %q, want %q", got, posixAbs)
		}
	}
}

func TestResolveClusterVolumeWindowsDrive(t *testing.T) {
	if runtime.GOOS != "windows" {
		t.Skip("Windows drive-letter paths are only absolute on Windows")
	}
	configDir := `C:\Users\user\proj`
	// A Windows drive-letter source must be recognized as absolute and left
	// alone, with its drive colon not mistaken for the SOURCE:DEST separator.
	spec := `C:\data:/workspace@server:*`
	if got := resolveClusterVolume(spec, configDir); got != spec {
		t.Errorf("windows absolute path mangled: got %q, want %q", got, spec)
	}
	// And a relative source still resolves against configDir.
	rel := `..:/workspace@server:*`
	want := filepath.Join(configDir, "..") + ":/workspace@server:*"
	if got := resolveClusterVolume(rel, configDir); got != want {
		t.Errorf("windows relative path: got %q, want %q", got, want)
	}
}

func TestSplitVolumeSpec(t *testing.T) {
	tests := []struct {
		in       string
		src, dst string
		ok       bool
	}{
		{"../:/workspace", "../", "/workspace", true},
		{"pgdata:/data", "pgdata", "/data", true},
		{`C:\path:/dest`, `C:\path`, "/dest", true},
		{"noseparator", "", "", false},
	}
	for _, tt := range tests {
		src, dst, ok := splitVolumeSpec(tt.in)
		if src != tt.src || dst != tt.dst || ok != tt.ok {
			t.Errorf("splitVolumeSpec(%q) = (%q,%q,%v), want (%q,%q,%v)",
				tt.in, src, dst, ok, tt.src, tt.dst, tt.ok)
		}
	}
}

func TestClusterAlreadyExists(t *testing.T) {
	cases := []struct {
		name string
		out  string
		want bool
	}{
		{"k3d message", "FATA[0000] Failed to create cluster 'devrig-x' because a cluster with that name already exists", true},
		{"case insensitive", "Cluster Already Exists", true},
		{"unrelated error", "docker daemon not running", false},
		{"empty", "", false},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if got := clusterAlreadyExists(c.out); got != c.want {
				t.Errorf("clusterAlreadyExists(%q) = %v, want %v", c.out, got, c.want)
			}
		})
	}
}

func TestExtractJSON(t *testing.T) {
	cases := []struct{ in, want string }{
		{`[{"name":"x"}]`, `[{"name":"x"}]`},
		{"WARN[0000] something\n[{\"name\":\"x\"}]", `[{"name":"x"}]`},
		{"no json here", "no json here"},
		{`{"a":1}`, `{"a":1}`},
	}
	for _, c := range cases {
		if got := extractJSON(c.in); got != c.want {
			t.Errorf("extractJSON(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}
