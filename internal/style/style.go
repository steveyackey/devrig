// Package style renders devrig's terminal output: colors, glyphs, and boxes
// that degrade gracefully. Color is used only on a capable TTY (respecting
// NO_COLOR / FORCE_COLOR / TERM=dumb, and enabling VT processing on Windows),
// and Unicode box/▮ glyphs fall back to ASCII where they may not render.
package style

import (
	"os"
	"regexp"
	"runtime"
	"strings"
	"unicode/utf8"

	"github.com/mattn/go-isatty"
)

var (
	colorOn   bool
	unicodeOn bool
)

func init() {
	colorOn = detectColor()
	unicodeOn = detectUnicode()
}

func detectColor() bool {
	if os.Getenv("NO_COLOR") != "" {
		return false
	}
	if os.Getenv("FORCE_COLOR") != "" {
		return true
	}
	if os.Getenv("TERM") == "dumb" {
		return false
	}
	fd := os.Stdout.Fd()
	if !isatty.IsTerminal(fd) && !isatty.IsCygwinTerminal(fd) {
		return false
	}
	if runtime.GOOS == "windows" {
		// Color only works once VT processing is enabled on the console.
		return enableVirtualTerminal()
	}
	return true
}

func detectUnicode() bool {
	if runtime.GOOS == "windows" {
		// Windows Terminal / VS Code render UTF-8 box glyphs; classic conhost
		// is unreliable, so default to ASCII unless we detect a modern host.
		return os.Getenv("WT_SESSION") != "" ||
			os.Getenv("WT_PROFILE_ID") != "" ||
			os.Getenv("TERM_PROGRAM") == "vscode"
	}
	for _, k := range []string{"LC_ALL", "LC_CTYPE", "LANG"} {
		if v := strings.ToUpper(os.Getenv(k)); v != "" {
			return strings.Contains(v, "UTF-8") || strings.Contains(v, "UTF8")
		}
	}
	return true // assume modern UTF-8 terminal
}

// ColorEnabled reports whether ANSI color is active.
func ColorEnabled() bool { return colorOn }

// --- color ---

func paint(s string, codes ...string) string {
	if !colorOn || len(codes) == 0 || s == "" {
		return s
	}
	return "\x1b[" + strings.Join(codes, ";") + "m" + s + "\x1b[0m"
}

func Bold(s string) string    { return paint(s, "1") }
func Dim(s string) string     { return paint(s, "2") }
func Cyan(s string) string    { return paint(s, "96") }
func Blue(s string) string    { return paint(s, "94") }
func Green(s string) string   { return paint(s, "92") }
func Yellow(s string) string  { return paint(s, "93") }
func Red(s string) string     { return paint(s, "91") }
func Magenta(s string) string { return paint(s, "95") }
func Gray(s string) string    { return paint(s, "90") }

func BoldCyan(s string) string    { return paint(s, "1", "96") }
func BoldMagenta(s string) string { return paint(s, "1", "95") }

// Link renders a URL, using an OSC 8 hyperlink on capable terminals so it's
// clickable, and underlined cyan otherwise.
func Link(url string) string {
	if !colorOn {
		return url
	}
	styled := paint(url, "4", "96")
	if runtime.GOOS == "windows" {
		return styled // OSC 8 support is spotty on Windows consoles
	}
	return "\x1b]8;;" + url + "\x1b\\" + styled + "\x1b]8;;\x1b\\"
}

// --- glyphs ---

// Glyphs is the active glyph set (Unicode or ASCII fallback).
type Glyphs struct {
	TopLeft, TopRight, BotLeft, BotRight, H, V string
	Bullet, MidDot, Arrow                      string
	Running, Pending, Failed, Stopped, Check   string
}

var unicodeGlyphs = Glyphs{
	TopLeft: "╭", TopRight: "╮", BotLeft: "╰", BotRight: "╯", H: "─", V: "│",
	Bullet: "◦", MidDot: "·", Arrow: "→",
	Running: "●", Pending: "◐", Failed: "✗", Stopped: "○", Check: "✓",
}

var asciiGlyphs = Glyphs{
	TopLeft: "+", TopRight: "+", BotLeft: "+", BotRight: "+", H: "-", V: "|",
	Bullet: "-", MidDot: "-", Arrow: "->",
	Running: "*", Pending: "o", Failed: "x", Stopped: ".", Check: "ok",
}

// G returns the active glyph set.
func G() Glyphs {
	if unicodeOn {
		return unicodeGlyphs
	}
	return asciiGlyphs
}

// --- width-aware helpers ---

var ansiRe = regexp.MustCompile(`\x1b\[[0-9;]*m|\x1b\]8;;[^\x1b]*\x1b\\`)

// Width returns the visible (printable) width of s, ignoring ANSI escapes.
func Width(s string) int {
	return utf8.RuneCountInString(ansiRe.ReplaceAllString(s, ""))
}

// PadRight pads s with spaces to a visible width of n.
func PadRight(s string, n int) string {
	if w := Width(s); w < n {
		return s + strings.Repeat(" ", n-w)
	}
	return s
}

// Box renders rows inside a rounded, titled border, indented two spaces. Rows
// may contain color escapes; widths are measured visibly so borders align.
// Falls back to ASCII corners when Unicode is unavailable.
func Box(title string, rows []string) string {
	gl := G()
	const padL, padR, indent = 2, 1, "  "
	inner := 4 + Width(title) // room for "─ title ─"
	for _, r := range rows {
		if w := padL + Width(r) + padR; w > inner {
			inner = w
		}
	}
	rep := func(n int) string {
		if n < 1 {
			n = 1
		}
		return strings.Repeat(gl.H, n)
	}

	var b strings.Builder
	// Top: ╭─ title ───╮
	tFill := inner - 3 - Width(title)
	b.WriteString(indent +
		Gray(gl.TopLeft+gl.H+" ") + BoldMagenta(title) + Gray(" "+rep(tFill)+gl.TopRight) + "\n")
	// Rows
	for _, r := range rows {
		fill := inner - padL - Width(r)
		if fill < 0 {
			fill = 0
		}
		b.WriteString(indent + Gray(gl.V) + strings.Repeat(" ", padL) + r + strings.Repeat(" ", fill) + Gray(gl.V) + "\n")
	}
	// Bottom: ╰────╯
	b.WriteString(indent + Gray(gl.BotLeft+rep(inner)+gl.BotRight) + "\n")
	return b.String()
}
