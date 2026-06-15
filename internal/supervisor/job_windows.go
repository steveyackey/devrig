//go:build windows

package supervisor

import (
	"os"
	"sync"
	"unsafe"

	"golang.org/x/sys/windows"
)

// Windows has no POSIX process groups or pdeathsig. A Job Object with
// JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE is the robust equivalent: every service is
// assigned to one job, and when the devrig process exits for ANY reason —
// clean shutdown, crash, or kill — the OS closes the job handle and terminates
// every process in it. This guarantees no orphaned services even if devrig
// itself dies abruptly.
var (
	jobOnce   sync.Once
	jobHandle windows.Handle
	jobErr    error
)

func ensureJob() (windows.Handle, error) {
	jobOnce.Do(func() {
		h, err := windows.CreateJobObject(nil, nil)
		if err != nil {
			jobErr = err
			return
		}
		info := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{
			BasicLimitInformation: windows.JOBOBJECT_BASIC_LIMIT_INFORMATION{
				LimitFlags: windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
			},
		}
		if _, err := windows.SetInformationJobObject(
			h,
			windows.JobObjectExtendedLimitInformation,
			uintptr(unsafe.Pointer(&info)),
			uint32(unsafe.Sizeof(info)),
		); err != nil {
			_ = windows.CloseHandle(h)
			jobErr = err
			return
		}
		// Intentionally never closed: the handle stays open for the lifetime of
		// the devrig process so the kill-on-close limit fires on exit.
		jobHandle = h
	})
	return jobHandle, jobErr
}

// assignToJob places a freshly started process into the kill-on-close job.
func assignToJob(p *os.Process) error {
	h, err := ensureJob()
	if err != nil {
		return err
	}
	ph, err := windows.OpenProcess(windows.PROCESS_SET_QUOTA|windows.PROCESS_TERMINATE, false, uint32(p.Pid))
	if err != nil {
		return err
	}
	defer windows.CloseHandle(ph)
	return windows.AssignProcessToJobObject(h, ph)
}
