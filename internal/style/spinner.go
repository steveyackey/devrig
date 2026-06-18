package style

import (
	"fmt"
	"os"
	"time"

	"github.com/steveyackey/devrig/internal/verbose"
)

// Spinner shows an animated "working…" indicator for a long step. It animates
// in place (carriage return) only on a color-capable TTY; otherwise it prints a
// single static line at start so piped/CI output stays clean. Stop with Done /
// Fail to replace the line with a final ✓/✗.
type Spinner struct {
	label  string
	frames []string
	stop   chan struct{}
	done   chan struct{}
	live   bool
}

// NewSpinner creates a spinner with the given label (no trailing punctuation).
func NewSpinner(label string) *Spinner {
	frames := []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}
	if !unicodeOn {
		frames = []string{"-", "\\", "|", "/"}
	}
	return &Spinner{label: label, frames: frames}
}

// Start begins the animation (or prints a static line when not on a TTY, or
// when verbose mode is streaming subprocess output that an in-place animation
// would otherwise scramble).
func (s *Spinner) Start() {
	if !colorOn || verbose.Enabled() {
		fmt.Fprintf(os.Stderr, "  %s…\n", s.label)
		return
	}
	s.live = true
	s.stop = make(chan struct{})
	s.done = make(chan struct{})
	go func() {
		defer close(s.done)
		fmt.Fprint(os.Stderr, "\x1b[?25l") // hide cursor
		t := time.NewTicker(90 * time.Millisecond)
		defer t.Stop()
		i := 0
		for {
			select {
			case <-s.stop:
				return
			case <-t.C:
				fmt.Fprintf(os.Stderr, "\r  %s %s ", Cyan(s.frames[i%len(s.frames)]), s.label)
				i++
			}
		}
	}()
}

func (s *Spinner) finish(glyph, msg string) {
	// Verbose mode never animated, so just print the final glyph + message on
	// its own line (color codes are stripped automatically when color is off).
	if verbose.Enabled() {
		if msg != "" {
			fmt.Fprintf(os.Stderr, "  %s %s\n", glyph, msg)
		}
		return
	}
	if !colorOn {
		if msg != "" {
			fmt.Fprintf(os.Stderr, "  %s\n", msg)
		}
		return
	}
	if s.live {
		close(s.stop)
		<-s.done
		s.live = false
	}
	// Clear the line, print the final state, restore the cursor.
	fmt.Fprintf(os.Stderr, "\r\x1b[K  %s %s\x1b[?25h\n", glyph, msg)
}

// Done replaces the spinner with a green check and msg (defaults to the label).
func (s *Spinner) Done(msg string) {
	if msg == "" {
		msg = s.label
	}
	s.finish(Green(G().Check), msg)
}

// Fail replaces the spinner with a red mark and msg (defaults to the label).
func (s *Spinner) Fail(msg string) {
	if msg == "" {
		msg = s.label
	}
	s.finish(Red(G().Failed), Red(msg))
}
