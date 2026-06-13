//go:build windows

package state

import (
	"os"

	"golang.org/x/sys/windows"
)

// lockFile takes an exclusive lock on the first byte range of f via
// LockFileEx (blocking) — the Windows analogue of flock(LOCK_EX).
func lockFile(f *os.File) {
	ol := &windows.Overlapped{}
	_ = windows.LockFileEx(windows.Handle(f.Fd()), windows.LOCKFILE_EXCLUSIVE_LOCK, 0, 1, 0, ol)
}

// unlockFile releases the lock taken by lockFile.
func unlockFile(f *os.File) {
	ol := &windows.Overlapped{}
	_ = windows.UnlockFileEx(windows.Handle(f.Fd()), 0, 1, 0, ol)
}
