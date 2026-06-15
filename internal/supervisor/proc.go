package supervisor

import (
	"time"

	"github.com/shirou/gopsutil/v4/process"
)

// ProcStartTimeMs returns a process's creation time (milliseconds since the
// Unix epoch), or ok=false if it can't be inspected (gone, or no permission).
// Combined with the PID it forms a reuse-proof identity: a recycled PID belongs
// to a process with a different start time.
func ProcStartTimeMs(pid int) (int64, bool) {
	p, err := process.NewProcess(int32(pid))
	if err != nil {
		return 0, false
	}
	ct, err := p.CreateTime()
	if err != nil {
		return 0, false
	}
	return ct, true
}

// SameProcess reports whether pid currently refers to the same process that had
// the given start time, guarding against PID reuse. wantStartMs == 0 means the
// start time is unknown (e.g. state written by an older devrig): fall back to a
// liveness-only check so behaviour degrades gracefully rather than refusing.
func SameProcess(pid int, wantStartMs int64) bool {
	got, ok := ProcStartTimeMs(pid)
	if !ok {
		return false
	}
	return wantStartMs == 0 || got == wantStartMs
}

// KillTree terminates pid and all of its descendants. Descendants are collected
// before any signal is sent (killing the parent reparents survivors, losing the
// tree). Each process gets a graceful Terminate, then a forceful Kill after
// grace if still alive. Cross-platform via gopsutil — used to reap orphaned
// service processes left by a crashed devrig, where no process-group handle
// from this run is available.
func KillTree(pid int, grace time.Duration) {
	root, err := process.NewProcess(int32(pid))
	if err != nil {
		return
	}

	var all []*process.Process
	var collect func(p *process.Process)
	collect = func(p *process.Process) {
		children, _ := p.Children()
		for _, c := range children {
			collect(c)
			all = append(all, c)
		}
	}
	collect(root)
	all = append(all, root) // children first, root last

	for _, p := range all {
		_ = p.Terminate()
	}
	time.Sleep(grace)
	for _, p := range all {
		if running, _ := p.IsRunning(); running {
			_ = p.Kill()
		}
	}
}
