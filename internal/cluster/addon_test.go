package cluster

import (
	"reflect"
	"testing"
)

func TestExpandDottedKeys(t *testing.T) {
	in := map[string]any{
		"image.repository": "reg/theoven",
		"image.tag":        "123",
		"replicaCount":     int64(2),
		"redis.enabled":    true,
	}
	got := expandDottedKeys(in)
	want := map[string]any{
		"image":        map[string]any{"repository": "reg/theoven", "tag": "123"},
		"replicaCount": int64(2),
		"redis":        map[string]any{"enabled": true},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("expandDottedKeys mismatch:\n got: %#v\nwant: %#v", got, want)
	}
}

func TestExpandDottedKeysPreservesNestedMaps(t *testing.T) {
	// A value that's already a nested map (from nested TOML tables) is kept as-is.
	in := map[string]any{
		"image":   map[string]any{"tag": "123"},
		"a.b.c":   "deep",
		"flatKey": "v",
	}
	got := expandDottedKeys(in)
	if img := got["image"].(map[string]any); img["tag"] != "123" {
		t.Errorf("nested image map not preserved: %#v", got["image"])
	}
	abc := got["a"].(map[string]any)["b"].(map[string]any)["c"]
	if abc != "deep" {
		t.Errorf("a.b.c = %v, want deep", abc)
	}
	if got["flatKey"] != "v" {
		t.Errorf("flatKey = %v", got["flatKey"])
	}
}
