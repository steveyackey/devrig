package main

import (
	"fmt"
	"os"
	"runtime/pprof"
	"runtime/trace"
	"strconv"
	"time"
)

// startStartupProfiling is a no-op unless DEVRIG_CPUPROFILE or DEVRIG_TRACE is
// set, so it costs nothing in normal use. When enabled it captures a CPU
// profile and/or execution trace for a short window (DEVRIG_PROFILE_MS, default
// 750ms) — long enough to cover process startup — then writes the files. It's a
// development/diagnostics hook for analyzing startup cost.
func startStartupProfiling() {
	cpuPath := os.Getenv("DEVRIG_CPUPROFILE")
	tracePath := os.Getenv("DEVRIG_TRACE")
	if cpuPath == "" && tracePath == "" {
		return
	}

	window := 750 * time.Millisecond
	if ms := os.Getenv("DEVRIG_PROFILE_MS"); ms != "" {
		if n, err := strconv.Atoi(ms); err == nil && n > 0 {
			window = time.Duration(n) * time.Millisecond
		}
	}

	var stop []func()
	if cpuPath != "" {
		f, err := os.Create(cpuPath)
		if err == nil {
			_ = pprof.StartCPUProfile(f)
			stop = append(stop, func() { pprof.StopCPUProfile(); _ = f.Close() })
		}
	}
	if tracePath != "" {
		f, err := os.Create(tracePath)
		if err == nil {
			_ = trace.Start(f)
			stop = append(stop, func() { trace.Stop(); _ = f.Close() })
		}
	}
	if len(stop) == 0 {
		return
	}

	go func() {
		time.Sleep(window)
		for _, s := range stop {
			s()
		}
		fmt.Fprintf(os.Stderr, "[profile] captured %s of startup; profiles written\n", window)
	}()
}
