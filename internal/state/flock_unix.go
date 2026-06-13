//go:build !windows

package state

import (
	"os"
	"syscall"
)

// lockFile takes an exclusive advisory lock on f (blocking).
func lockFile(f *os.File) {
	_ = syscall.Flock(int(f.Fd()), syscall.LOCK_EX)
}

// unlockFile releases the advisory lock on f.
func unlockFile(f *os.File) {
	_ = syscall.Flock(int(f.Fd()), syscall.LOCK_UN)
}
