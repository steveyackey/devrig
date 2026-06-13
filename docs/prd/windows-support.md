# PRD: Windows Support

## Status

Partially superseded — recommend human review.

Much of this PRD's original premise has since been implemented during the Go
rewrite, via per-package build-tag files rather than the single centralized
platform module proposed below:

- **Shell selection** — the supervisor already runs commands through `cmd /C`
  on Windows and `sh -c` elsewhere (`internal/supervisor/supervisor.go`).
- **Process-group / signal handling** — split across build-tag files
  (`internal/orchestrator/signal_{unix,windows}.go`,
  `internal/supervisor/process_{unix,windows}.go`).
- **Process liveness** — handled per-platform in the same build-tag files.
- **Home directory** — already cross-platform via `os.UserHomeDir()`
  (`internal/registry/registry.go`).
- **CI** — Windows (`windows-latest`) already runs `go build`, `go test`, and
  `go vet` in `.github/workflows/ci.yml`.

**Still open:** graceful Windows shutdown (the current Windows path is a hard
`Process.Kill()`, not a SIGTERM-equivalent with a grace period) and port-owner
identification (devrig currently reports only "port N in use", with no owning
process name, on every platform). The sections below are kept for historical
context and as a backlog for the remaining work; treat the implementation
details as illustrative of intent, not as the current code.

## Problem Statement

The Go rewrite uses Unix-specific syscalls for process lifecycle management
(`syscall.SIGTERM`, process groups via `Setpgid`, `Kill(-pgid, …)`). The
Windows build-tag files currently fall back to a hard `Process.Kill()` with no
graceful shutdown, and port-owner identification is not implemented on any
platform. The goal of this PRD is full Windows feature parity: graceful
shutdown of process trees and richer port-conflict diagnostics.

Windows is the most popular desktop OS for developers. The 2024 Stack Overflow
Developer Survey shows ~60% of professional developers use Windows as their
primary OS. Supporting Windows natively removes a hard blocker for adoption in
teams where not everyone runs Linux or macOS.

## Scope

**Primary effort:** Native Windows support -- devrig compiles and runs
correctly on Windows without WSL.

**Secondary effort:** Document WSL2 as the zero-friction path for users who
prefer a Linux environment on Windows.

### In scope

- Fix all 6 identified platform gaps (detailed below)
- Windows CI (compile + unit tests + integration tests)
- WSL2 getting-started guide
- Per-platform build-tag files (`*_windows.go` / `*_unix.go`) where APIs
  diverge, sharing code where they don't

### Out of scope

- GUI or Windows service (SCM) integration
- PowerShell module or winget package (future work)
- Windows containers (devrig uses Docker/k3d which run Linux containers via
  Docker Desktop or Rancher Desktop on Windows)

## Platform Audit

Six areas of the codebase contain Unix-specific code that must be ported.

### 1. Shell execution (`internal/supervisor/supervisor.go`)

**Current:** `exec.Command("sh", "-c", command)` on Unix; `exec.Command("cmd",
"/C", command)` on Windows (already implemented).

**Impact:** Resolved — services start on Windows via `cmd /C`.

### 2. Process group lifecycle (`internal/supervisor/process_{unix,windows}.go`)

**Current:**
- Unix: `Setpgid` puts the process in its own group; `Kill(-pgid, SIGTERM)`
  with a 5-second grace period then `SIGKILL`.
- Windows: `setProcGroup` is a no-op and `terminateProcess` calls
  `Process.Kill()` with no graceful shutdown.

**Impact:** On Windows, services and their child processes cannot be terminated
gracefully. Orphaned descendant processes may leak on stop/Ctrl+C.

### 3. Process liveness detection (`internal/orchestrator/signal_{unix,windows}.go`)

**Current:** Unix uses `Process.Signal(syscall.Signal(0))` (the `kill(pid, 0)`
idiom). The Windows `isAlive` falls back to returning `false`.

**Impact:** `devrig ps` may show services as "stopped" on Windows.

### 4. Home directory resolution (`internal/registry/registry.go`)

**Current:** `os.UserHomeDir()` — used to locate the global instance registry
at `~/.devrig/instances.json`.

**Impact:** Resolved — `os.UserHomeDir()` is cross-platform (it uses
`%USERPROFILE%` on Windows).

### 5. Port owner identification (`internal/ports/ports.go`)

**Current:** Not implemented on any platform. A port conflict is reported as
"port N required by … is already in use" without the owning process name.

**Impact:** Port-conflict messages lose the "in use by X (PID Y)" detail. A
Linux implementation (parsing `/proc/net/tcp`) and a Windows implementation
(IP Helper API) would both be new work.

### 6. Platform-specific syscalls (build tags)

**Current:** Unix signal handling lives in `*_unix.go` files behind a
`//go:build !windows` tag; the Windows counterparts live in `*_windows.go`
files behind `//go:build windows`. Go's standard `syscall`/`os` packages cover
both, so no third-party platform dependency is required.

**Impact:** `go build` already succeeds for Windows targets; the remaining work
is enriching the Windows code paths, not making them compile.

## Design

### 1. Shell execution

**Windows:** Use `cmd /C` as the shell wrapper (already implemented — the
supervisor selects `cmd /C` on Windows and `sh -c` elsewhere at runtime via a
`runtime.GOOS` check in `internal/supervisor/supervisor.go`).

**Alternative considered:** Always use `sh` and require Git Bash / MSYS2 on
PATH. Rejected -- introduces an undocumented dependency and breaks for users
who don't have Git installed.

**Alternative considered:** Use PowerShell (`pwsh -Command`). Rejected as the
default -- `cmd.exe` is universally available and has lower startup overhead.
A future `shell` config key could let users opt into PowerShell per-service.

### 2. Process group lifecycle

**Windows:** Use Win32 Job Objects to group a process and all its descendants,
so the whole tree can be terminated together. The remaining design:

1. Create a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, spawn the
   child, and assign it to the job.
2. For graceful shutdown, send `CTRL_BREAK_EVENT` to the process group
   (requires `CREATE_NEW_PROCESS_GROUP` on spawn), wait up to 5 seconds
   (matching the Unix grace period), then fall back to `TerminateJobObject`
   (the SIGKILL-equivalent for the whole job).

This would live in `internal/supervisor/process_windows.go`, replacing the
current hard `Process.Kill()`. Go can reach the Win32 APIs through
`golang.org/x/sys/windows`. `CTRL_BREAK_EVENT` is the Windows analogue of
SIGTERM for console apps; most Node.js, Python, and Go runtimes handle it.

### 3. Process liveness detection

**Windows:** Use `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION`, then
`GetExitCodeProcess`, treating `STILL_ACTIVE` as alive. This would replace the
current `isAlive` fallback in the Windows build-tag file.

### 4. Home directory resolution

**Resolved.** `internal/registry/registry.go` already uses `os.UserHomeDir()`,
which resolves `%USERPROFILE%` on Windows and `$HOME` on Unix, and joins
`.devrig/instances.json`. No further change is needed.

### 5. Port owner identification

This is unimplemented on every platform today. To enrich port-conflict
messages with the owning process name:

- **Linux:** parse `/proc/net/tcp` and the `/proc/<pid>/fd` symlinks.
- **Windows:** use `GetExtendedTcpTable` (IP Helper API) to map the local port
  to a PID, then `OpenProcess` + `QueryFullProcessImageNameW` for the exe name.
- **macOS:** shell out to `lsof -i :<port> -P -n` (or use `libproc`).

Each would be a per-platform build-tag file under `internal/ports/`.

### 6. Platform-specific syscalls

No third-party platform crate is needed in Go: the standard `os`, `syscall`,
and (for richer Win32 access) `golang.org/x/sys/windows` packages cover the
required APIs. Divergent code is already isolated in `*_unix.go` /
`*_windows.go` files guarded by `//go:build` tags, so each platform compiles
only its own implementation.

### Platform boundary

Rather than a single centralized platform module, devrig keeps each platform
detail next to the package that needs it, in paired build-tag files
(`signal_{unix,windows}.go`, `process_{unix,windows}.go`, and any future
`ports/*_{unix,windows}.go`). The shared code calls one function name
(`sendStop`, `terminateProcess`, `isAlive`, …) and the build tag selects the
right implementation. This keeps the platform boundary explicit and each file
testable on its own OS.

## Testing Strategy

### CI matrix

The CI workflow already runs a `windows-latest` runner alongside
`ubuntu-latest`. A `macos-latest` runner could be added for full coverage:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
```

### Test tiers

| Tier | What | Runs on |
|------|------|---------|
| Compile | `go build ./...` | All 3 OSes |
| Unit / `go test` | `go test ./...`, `go vet ./...` | All 3 OSes |
| Docker-dependent | tests that spawn containers | Linux + macOS (Docker required) |
| Windows-specific | process-tree shutdown assertions | Windows |

### Platform-specific test guards

Tests that exercise Unix signals must be guarded with build tags
(`//go:build !windows` / `//go:build windows`) or skipped at runtime via
`runtime.GOOS`, with a Windows equivalent (e.g. `taskkill /PID`) where one is
needed. Tests that verify graceful shutdown (SIGTERM → wait → SIGKILL) would
gain Windows-specific assertions for the Job Object cleanup path once
implemented.

### Docker requirement

Some tests require Docker. On Windows CI, Docker Desktop is not available by
default on GitHub Actions `windows-latest`. Options:

1. **Skip Docker-dependent tests on Windows CI** (e.g. behind a build tag or
   a `runtime.GOOS`/env-var check) and run them manually or on a self-hosted
   runner.
2. **Use Docker-in-Docker** via a Windows container agent (complex, not
   recommended for initial rollout).

Recommendation: Option 1 for the initial release. Docker-dependent tests run on
Linux/macOS CI; Windows CI runs build + `go test` + non-Docker tests.

## Milestones

### M1: Compilation (no runtime) — done

**Goal:** `go build ./...` passes for Windows.

- Isolate platform syscalls in `*_unix.go` / `*_windows.go` build-tag files
- Add `windows-latest` to the CI matrix (build + `go test` + `go vet`)

**Status:** Already complete — the Go rewrite ships build-tag files and Windows
CI is green.

### M2: Core runtime

**Goal:** `devrig start` / `devrig stop` work cleanly on Windows.

- Shell selection via `cmd /C` (done)
- Implement Job Object process-group management in `process_windows.go`
- Implement `CTRL_BREAK_EVENT` graceful shutdown with a 5s timeout
- Implement a real `isAlive` for Windows via `OpenProcess`
- Cross-platform home directory via `os.UserHomeDir()` (done)
- Add Windows-specific unit tests

**Verification:** `devrig start` runs services, `Ctrl+C` stops them cleanly,
`devrig ps` shows correct status. Manual testing on a Windows machine.

### M3: Port diagnostics + polish

**Goal:** Full feature parity with Linux.

- Implement port-owner identification using `GetExtendedTcpTable`
- Verify Docker Desktop integration (container lifecycle, port mapping)
- Verify k3d integration on Windows
- Add Windows-specific integration tests to CI
- Update `devrig doctor` to check for Windows-specific prerequisites
  (Docker Desktop, `cmd.exe`)

**Verification:** `devrig doctor` passes on Windows. Port conflict messages
show process names. Integration test suite green.

### M4: Documentation + release

**Goal:** Windows users can install and use devrig.

- Write WSL2 getting-started guide (see below)
- Update main README with Windows installation instructions
- Add Windows to the release matrix (cross-compile or native CI build)
- Update `docs/guides/getting-started.md` with Windows notes

**Verification:** A fresh Windows machine can follow the docs and run
`devrig start` on a sample project.

## WSL2 Guide

Include a `docs/guides/wsl2.md` guide covering:

### Quick start

```bash
# Install WSL2 (PowerShell, admin)
wsl --install -d Ubuntu

# Inside WSL2: download the latest Linux release binary and put it on PATH,
# or `go install github.com/steveyackey/devrig/cmd/devrig@latest`
# (see docs/guides/getting-started.md for both options)

# Docker: Install Docker Desktop for Windows, enable WSL2 backend
# The `docker` CLI is automatically available inside WSL2
```

### Key points to document

1. **WSL2 is the easiest path.** devrig works in WSL2 exactly as it does on
   Linux -- no Windows-specific code paths are exercised. If you use WSL2 for
   development already, just install devrig inside WSL2 and you're done.

2. **Docker integration.** Docker Desktop's WSL2 backend makes the `docker`
   CLI available inside WSL2 without additional configuration. devrig's docker
   containers, compose integration, and k3d clusters all work as-is.

3. **File system performance.** Store projects on the Linux filesystem
   (`~/projects/`) not the Windows mount (`/mnt/c/`). The 9P mount is
   significantly slower and causes issues with file watchers.

4. **Port forwarding.** WSL2 automatically forwards ports to the Windows host.
   Services started via devrig inside WSL2 are accessible from Windows browsers
   at `localhost:<port>`.

5. **VS Code integration.** Use the "WSL" extension to open projects inside
   WSL2. The terminal runs in Linux, so devrig commands work natively.

6. **When to use native Windows instead.** Native Windows support (M2+) is for
   developers who don't use WSL2, or whose projects must run natively on
   Windows (e.g. .NET services, Windows-specific toolchains).
