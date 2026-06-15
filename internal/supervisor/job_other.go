//go:build !windows

package supervisor

import "os"

// assignToJob is a no-op off Windows. Unix relies on process groups (set in
// setProcGroup) plus startup reconciliation to clean up orphans.
func assignToJob(*os.Process) error { return nil }
