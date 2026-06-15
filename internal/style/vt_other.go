//go:build !windows

package style

// enableVirtualTerminal is Windows-only; elsewhere ANSI works natively.
func enableVirtualTerminal() bool { return true }
