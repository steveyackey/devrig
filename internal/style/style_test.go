package style

import (
	"strings"
	"testing"
)

func TestWidthStripsANSI(t *testing.T) {
	cases := map[string]int{
		"hello":                  5,
		"\x1b[1;96mhello\x1b[0m": 5,
		"\x1b[90m●\x1b[0m":       1,
		"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\": 4, // OSC8 hyperlink wrapping "link"
	}
	for in, want := range cases {
		if got := Width(in); got != want {
			t.Errorf("Width(%q) = %d, want %d", in, got, want)
		}
	}
}

func TestPadRight(t *testing.T) {
	if got := PadRight("ab", 5); got != "ab   " {
		t.Errorf("PadRight = %q", got)
	}
	// Padding accounts for invisible escapes.
	colored := "\x1b[1mab\x1b[0m"
	got := PadRight(colored, 5)
	if Width(got) != 5 {
		t.Errorf("PadRight visible width = %d, want 5", Width(got))
	}
}

// Every line of a rendered box must have the same visible width so borders align.
func TestBoxBordersAlign(t *testing.T) {
	out := Box("project", []string{
		"a short row",
		"a considerably longer row than the others here",
		"mid",
	})
	lines := strings.Split(strings.TrimRight(out, "\n"), "\n")
	if len(lines) != 5 { // top + 3 rows + bottom
		t.Fatalf("expected 5 lines, got %d", len(lines))
	}
	want := Width(lines[0])
	for i, l := range lines {
		if w := Width(l); w != want {
			t.Errorf("line %d visible width = %d, want %d (%q)", i, w, want, l)
		}
	}
}

func TestGlyphsSelectable(t *testing.T) {
	// Both glyph sets must be fully populated (no empty fields that would break layout).
	for _, g := range []Glyphs{unicodeGlyphs, asciiGlyphs} {
		if g.TopLeft == "" || g.BotRight == "" || g.V == "" || g.H == "" || g.Running == "" {
			t.Errorf("glyph set has empty fields: %+v", g)
		}
	}
}
