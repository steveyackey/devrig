# PRD: Windows Support

## Status

Proposed

## Problem Statement

devrig currently targets Linux and macOS exclusively. All process lifecycle
management uses Unix-specific APIs (`nix` crate, `sh -c`, `killpg`, process
groups, `/proc` filesystem), and the `#[cfg(not(unix))]` fallbacks are stubs
that return `false` or silently skip functionality. A developer on Windows
cannot use devrig at all -- services won't terminate gracefully, port-owner
identification is disabled, process liveness checks always return false, and
the home directory lookup uses `$HOME` which is not set by default on Windows.

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
- Conditional compilation with `#[cfg(windows)]` / `#[cfg(unix)]` where APIs
  diverge, sharing code where they don't

### Out of scope

- GUI or Windows service (SCM) integration
- PowerShell module or winget package (future work)
- Windows containers (devrig uses Docker/k3d which run Linux containers via
  Docker Desktop or Rancher Desktop on Windows)

## Platform Audit

Six areas of the codebase contain Unix-specific code that must be ported.

### 1. Shell execution (`supervisor.rs:100`)

**Current:** `Command::new("sh").arg("-c").arg(&self.command)`

Commands are spawned via `sh -c`, which does not exist natively on Windows.

**Impact:** Complete -- no service can start on Windows.

### 2. Process group lifecycle (`supervisor.rs:114-338`)

**Current:**
- `cmd.process_group(0)` creates a new Unix process group.
- `killpg(pgid, Signal::SIGTERM)` sends SIGTERM to the group.
- 5-second grace period, then `SIGKILL`.
- `#[cfg(not(unix))]` fallback calls `child.kill()` with no graceful shutdown.

**Impact:** Services and their child processes cannot be terminated gracefully.
Orphaned processes will leak on stop/Ctrl+C.

### 3. Process liveness detection (`commands/ps.rs:156-169`)

**Current:** `nix::sys::signal::kill(Pid, None)` (the Unix `kill(pid, 0)`
idiom). The `#[cfg(not(unix))]` branch returns `false` unconditionally.

**Impact:** `devrig ps` always shows all services as "stopped" on Windows.

### 4. Home directory resolution (`orchestrator/registry.rs:20`)

**Current:** `std::env::var("HOME").unwrap_or("/tmp")` -- used to locate the
global instance registry at `~/.devrig/instances.json`.

**Impact:** On Windows, `HOME` is usually unset. The fallback to `/tmp` is an
invalid path. The registry file will fail to write, breaking `devrig ps --all`
and multi-instance tracking.

### 5. Port owner identification (`orchestrator/ports.rs:89-150`)

**Current:** Reads `/proc/net/tcp` and `/proc/<pid>/fd` symlinks to map a port
to a process name. Gated on `#[cfg(target_os = "linux")]`. The
`#[cfg(not(target_os = "linux"))]` branch returns `None`.

**Impact:** Port-conflict error messages lose the "in use by X (PID Y)" detail
on both macOS and Windows. macOS already has this gap; Windows needs its own
implementation.

### 6. `nix` crate dependency (`Cargo.toml:18`)

**Current:** `nix = { version = "0.29", features = ["signal", "process"] }` is
an unconditional dependency. The `nix` crate does not compile on Windows.

**Impact:** `cargo build` fails on Windows targets.

## Design

### 1. Shell execution

**Windows:** Use `cmd.exe /C` as the shell wrapper.

```rust
#[cfg(unix)]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(script);
    cmd
}

#[cfg(windows)]
fn shell_command(script: &str) -> Command {
    let mut cmd = Command::new("cmd.exe");
    cmd.arg("/C").arg(script);
    cmd
}
```

Place this in a new `src/platform.rs` module that centralizes all
platform-specific helpers. The supervisor calls `platform::shell_command()`
instead of hard-coding `sh -c`.

**Alternative considered:** Always use `sh` and require Git Bash / MSYS2 on
PATH. Rejected -- introduces an undocumented dependency and breaks for users
who don't have Git installed.

**Alternative considered:** Use PowerShell (`pwsh -Command`). Rejected as the
default -- `cmd.exe` is universally available and has lower startup overhead.
A future `shell` config key could let users opt into PowerShell per-service.

### 2. Process group lifecycle

**Windows:** Use Win32 Job Objects to group a process and all its descendants.

```rust
#[cfg(windows)]
fn spawn_in_job(cmd: &mut Command) -> Result<(Child, JobHandle)> {
    // 1. CreateJobObjectW(NULL, NULL)
    // 2. SetInformationJobObject with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
    // 3. Spawn child
    // 4. AssignProcessToJobObject(job, child_handle)
}
```

For graceful shutdown:

1. Send `CTRL_BREAK_EVENT` via `GenerateConsoleCtrlEvent` to the process
   group (requires `CREATE_NEW_PROCESS_GROUP` on spawn).
2. Wait up to 5 seconds for exit (matching the Unix grace period).
3. Fall back to `TerminateJobObject` (equivalent of SIGKILL to the entire
   job, killing all descendants).

Use the `windows-sys` crate for raw Win32 FFI (zero-cost, official Microsoft
crate). Avoid pulling in the full `windows` crate to minimize compile time.

**Graceful shutdown detail:** `CTRL_BREAK_EVENT` is the Windows equivalent of
SIGTERM for console applications. Most Node.js, Python, and Go runtimes
handle it correctly. If the process doesn't handle it, the 5-second timeout
and `TerminateJobObject` ensure cleanup.

### 3. Process liveness detection

**Windows:** Use `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION` access,
then check the result.

```rust
#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use windows_sys::Win32::System::Threading::*;
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        let mut exit_code: u32 = 0;
        let alive = GetExitCodeProcess(handle, &mut exit_code) != 0
            && exit_code == STILL_ACTIVE;
        CloseHandle(handle);
        alive
    }
}
```

### 4. Home directory resolution

**Cross-platform fix:** Replace the `$HOME` lookup with `dirs::home_dir()` from
the `dirs` crate (or the equivalent `std::env::var("USERPROFILE")` on Windows).

The simplest and most portable approach:

```rust
fn devrig_home() -> PathBuf {
    // Works on Linux ($HOME), macOS ($HOME), Windows (%USERPROFILE%)
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".devrig")
}
```

This fixes the existing code for all platforms, not just Windows. The `dirs`
crate is a lightweight, well-maintained dependency (~30 LOC, no transitive
deps).

### 5. Port owner identification

**Windows:** Use `GetExtendedTcpTable` from the IP Helper API to map a local
port to a PID, then `OpenProcess` + `QueryFullProcessImageNameW` to get the
process name.

```rust
#[cfg(windows)]
pub fn identify_port_owner(port: u16) -> Option<String> {
    // 1. GetExtendedTcpTable(AF_INET, TCP_TABLE_OWNER_PID_LISTENER)
    //    -> MIB_TCPTABLE_OWNER_PID rows
    // 2. Find row where dwLocalPort == port
    // 3. OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, row.dwOwningPid)
    // 4. QueryFullProcessImageNameW -> exe path
    // 5. Return "notepad.exe (PID 1234)" style string
}
```

This also fills the existing gap on macOS, where `identify_port_owner` currently
returns `None`. A macOS implementation using `lsof -i :<port> -P -n` (or
`libproc`) can be added in the same milestone but is not required for this PRD.

### 6. `nix` crate conditionalization

**Fix:** Make `nix` a Unix-only dependency and add `windows-sys` for Windows.

```toml
[target.'cfg(unix)'.dependencies]
nix = { version = "0.29", features = ["signal", "process"] }

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_System_Threading",
    "Win32_System_Console",
    "Win32_System_JobObjects",
    "Win32_Security",
    "Win32_NetworkManagement_IpHelper",
    "Win32_Foundation",
] }
```

All `use nix::` imports are already behind `#[cfg(unix)]` in the source. The
only change needed in `Cargo.toml` is moving the dependency under
`[target.'cfg(unix)'.dependencies]`.

### New module: `src/platform.rs`

All platform-specific code is consolidated into a single module with a clean
internal API:

```rust
// src/platform.rs

/// Build a Command that runs `script` through the platform shell.
pub fn shell_command(script: &str) -> Command;

/// Check if a process is still alive.
pub fn is_process_alive(pid: u32) -> bool;

/// Return the user's home directory.
pub fn home_dir() -> PathBuf;

/// Identify which process owns a given TCP port.
pub fn identify_port_owner(port: u16) -> Option<String>;
```

The supervisor and other modules call into `platform::*` instead of using
inline `#[cfg]` blocks scattered throughout the codebase. This makes the
platform boundary explicit and testable.

## Testing Strategy

### CI matrix

Add a `windows-latest` runner to the GitHub Actions CI workflow:

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
```

### Test tiers

| Tier | What | Runs on |
|------|------|---------|
| Compile | `cargo check --all-targets` | All 3 OSes |
| Unit tests | `cargo test` (no integration feature) | All 3 OSes |
| Integration | `cargo test --features integration` | Linux + macOS (Docker required) |
| Windows integration | Subset of integration tests using `#[cfg(windows)]` | Windows (Docker Desktop required) |

### Platform-specific test guards

Integration tests that send Unix signals (`nix::sys::signal::kill`) need
Windows equivalents or must be skipped:

```rust
#[cfg(unix)]
{
    nix::sys::signal::kill(pid, Signal::SIGINT);
}
#[cfg(windows)]
{
    // Use GenerateConsoleCtrlEvent or taskkill
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .status()
        .unwrap();
}
```

Tests that verify graceful shutdown behavior (SIGTERM -> wait -> SIGKILL) will
have Windows-specific assertions that verify the Job Object cleanup path.

### Docker requirement

Integration tests require Docker. On Windows CI, Docker Desktop is not
available by default on GitHub Actions `windows-latest`. Options:

1. **Skip Docker-dependent tests on Windows CI** with
   `#[cfg_attr(windows, ignore)]` and run them manually or in a self-hosted
   runner.
2. **Use Docker-in-Docker** via a Windows container agent (complex, not
   recommended for initial rollout).

Recommendation: Option 1 for the initial release. Docker-dependent integration
tests run on Linux/macOS CI; Windows CI runs compile + unit + non-Docker
integration tests.

## Milestones

### M1: Compilation (no runtime)

**Goal:** `cargo check --target x86_64-pc-windows-msvc` passes.

- Move `nix` to `[target.'cfg(unix)'.dependencies]`
- Add `windows-sys` to `[target.'cfg(windows)'.dependencies]`
- Create `src/platform.rs` with stub implementations for Windows
- Add `windows-latest` to CI matrix (compile-only)

**Verification:** CI green on all 3 OSes.

### M2: Core runtime

**Goal:** `devrig start` / `devrig stop` work on Windows.

- Implement `platform::shell_command()` using `cmd.exe /C`
- Implement Job Object process group management
- Implement `CTRL_BREAK_EVENT` graceful shutdown with 5s timeout
- Implement `platform::is_process_alive()` via `OpenProcess`
- Fix home directory with `dirs::home_dir()`
- Port unit tests, add Windows-specific unit tests

**Verification:** `devrig start` runs services, `Ctrl+C` stops them cleanly,
`devrig ps` shows correct status. Manual testing on a Windows machine.

### M3: Port diagnostics + polish

**Goal:** Full feature parity with Linux.

- Implement `identify_port_owner` using `GetExtendedTcpTable`
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

# Inside WSL2:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install devrig

# Docker: Install Docker Desktop for Windows, enable WSL2 backend
# The `docker` CLI is automatically available inside WSL2
```

### Key points to document

1. **WSL2 is the easiest path.** devrig works in WSL2 exactly as it does on
   Linux -- no Windows-specific code paths are exercised. If you use WSL2 for
   development already, just install devrig inside WSL2 and you're done.

2. **Docker integration.** Docker Desktop's WSL2 backend makes the `docker`
   CLI available inside WSL2 without additional configuration. devrig's infra
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
