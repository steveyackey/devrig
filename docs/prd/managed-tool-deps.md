# PRD: Managed Tool Dependencies (k3d / kubectl / helm)

## Status

Proposed — implemented in the same change set. This document is the design of
record; the implementation lives in `internal/tools/` and is wired into the
cluster code paths.

## Problem Statement

devrig's cluster features shell out to three external CLIs — `k3d`, `kubectl`,
and `helm` — by bare name on `$PATH` (`internal/cluster/addon.go`,
`internal/cluster/k3d.go`, `internal/commands/kubectl.go`). `devrig doctor`
only checks whether each is present. This creates two problems:

1. **Onboarding friction.** A new user who wants the cluster workflow must
   manually install three tools at the right versions before `devrig cluster
   create` works. The error today is a raw `exec: "k3d": executable file not
   found in $PATH`.
2. **Non-determinism.** Whatever version happens to be on the user's `$PATH` is
   used. Version skew (e.g. a `kubectl` several minors away from the k3s the
   cluster runs, or an old `k3d` missing a feature devrig relies on) produces
   "works on my machine" failures — exactly what a local-environment
   orchestrator exists to eliminate.

Only **Docker is genuinely required** for devrig; k3d/kubectl/helm are optional
and only needed for the cluster feature. So the fix should not bloat every
install — it should provide the tools, at known-good versions, *when the
cluster feature is actually used*.

## Goals

- devrig can run the cluster workflow on a machine that has **none** of
  k3d/kubectl/helm pre-installed (Docker still required).
- The versions devrig uses are **pinned and tested in CI**, giving a
  reproducible, validated toolchain.
- Managed binaries are **isolated** from the user's `$PATH` — they never shadow
  or clobber the user's own k3d/kubectl/helm (and their krew plugins, etc.).
- A clear, documented **precedence** between managed and system tools, with an
  escape hatch for users who want their own.
- Downloads are **integrity-checked** against pinned SHA-256 checksums.

## Non-Goals

- Removing the Docker dependency (impossible — k3d runs k3s in containers).
- Embedding these tools as Go libraries (heavier dep tree / binary size /
  upgrade treadmill; revisit helm's SDK separately). v1 fetches binaries.
- Dynamically matching `kubectl` to the running cluster's k8s version. We pin a
  single tested `kubectl`; cluster-version matching is a future enhancement.
- Windows/arm64 managed tools (k3d publishes no windows/arm64 binary). devrig
  still runs there; the cluster feature just needs the user to supply tools.

## Design

### Install location & isolation

Managed binaries live in a **devrig-private** directory, not the user's
`$PATH`:

```
~/.devrig/bin/<tool>-<version>[.exe]
```

devrig invokes them by **absolute path**. Nothing is added to the user's
interactive `$PATH`. (Contrast with devrig's *own* binary, which the installer
puts in `~/.local/bin` — that is intentional and unchanged.) Versioned
filenames let multiple pinned versions coexist and make "is the pinned version
present?" a simple `os.Stat`.

### Pinned versions & checksums

A single source of truth, `scripts/update-tool-checksums.sh`, holds the pinned
versions and regenerates `internal/tools/checksums_gen.go` with the SHA-256 of
every artifact across all supported platforms. To bump a tool: edit the version
in the script, re-run it, commit. CI re-downloads and re-verifies every pinned
checksum, so a bad pin fails the build, not a user's machine.

Pinned at authoring time: `kubectl 1.36.2`, `helm 3.21.1`, `k3d 5.9.0`.
(helm is pinned to 3.x deliberately: `addon.go` uses helm-3 `upgrade --install
--include-crds` idioms; moving to helm 4 needs a compatibility pass on those
arg lists first.)

Supported platforms (goos/goarch): `linux/{amd64,arm64}`,
`darwin/{amd64,arm64}`, `windows/amd64`.

### Download sources & formats

| Tool    | URL                                                                   | Artifact          |
|---------|-----------------------------------------------------------------------|-------------------|
| kubectl | `https://dl.k8s.io/release/v{ver}/bin/{os}/{arch}/kubectl[.exe]`       | raw binary        |
| helm    | `https://get.helm.sh/helm-v{ver}-{os}-{arch}.tar.gz`                   | tar.gz → `{os}-{arch}/helm[.exe]` |
| k3d     | `https://github.com/k3d-io/k3d/releases/download/v{ver}/k3d-{os}-{arch}[.exe]` | raw binary |

Fetch flow: download to a temp file → stream-compute SHA-256 → compare against
the pinned checksum (mismatch ⇒ hard fail, nothing installed) → for helm,
extract the binary from the tarball → `chmod 0755` → atomic rename into
`~/.devrig/bin/`.

### Resolution precedence

A `tools.Resolver` (configured from the `[tools]` config block) resolves a tool
to an absolute path. Default `prefer = "vendored"`:

1. Explicit per-tool path override (config) → use it (error if missing).
2. Managed binary for the pinned version present → use it.
3. Fetching allowed (interactive TTY, or `devrig deps install`) → fetch → use.
4. Fallback: a system binary on `$PATH` → use it (better than failing).
5. Otherwise: error with remediation (`devrig deps install`).

With `prefer = "system"`, steps 2 and 4 swap: system `$PATH` is tried before
the managed copy. In non-interactive contexts (no TTY) devrig never silently
downloads; it uses what's present or returns an actionable error.

```toml
[tools]
prefer = "vendored"            # or "system"
# kubectl = "/usr/local/bin/kubectl"   # explicit override, per tool
```

### "Vendored goes stale" — the update story

Managed ≠ frozen. The pinned versions are **the versions devrig was tested
against**, and they advance when devrig is updated:

- `devrig update` ships a binary whose pins may have moved; the next cluster
  command fetches the new pinned versions on demand.
- `devrig deps update` re-fetches the current pins out of band.

This couples tool updates to a devrig release that validated the combination —
a feature, not a limitation.

### CLI surface

- `devrig deps list` — show each tool: pinned version, what's installed
  (managed/system), and which devrig will use.
- `devrig deps install [tool...]` — fetch missing managed tools (all, or named).
- `devrig deps update [tool...]` — re-fetch pinned versions, overwriting.
- `devrig doctor` — augmented to show managed-vs-system status per tool instead
  of a bare present/absent check.

### Affected code

- New `internal/tools/` package: `Tool` type, `Resolver`, fetch/verify/extract,
  generated `checksums_gen.go`.
- `internal/config/model.go`: `[tools]` block (`prefer`, per-tool overrides).
- `internal/cluster/{addon,k3d}.go` and `internal/commands/kubectl.go`: replace
  bare `"kubectl"`/`"helm"`/`"k3d"` with resolver lookups.
- `internal/commands/`: new `deps` command; `doctor` enhancement.

## Testing & CI

- **Unit tests** (`go test ./...`, no network): platform→URL/artifact mapping,
  checksum-map completeness for every supported platform, resolver precedence
  (vendored/system/override) against a fake filesystem + PATH, tarball
  extraction, checksum-mismatch rejection.
- **Integration test** (build-tagged `toolsintegration`, network required):
  for each tool, fetch the pinned version, verify the checksum, run
  `<tool> version`, and assert the reported version matches the pin. This is the
  validation the user asked for — it proves the URLs, versions, and checksums
  are all correct and the binaries actually execute.
- **CI job** (`.github/workflows/ci.yml`): a `tool-deps` job runs the
  integration test on `ubuntu-latest` (and is safe to extend to a
  macOS/Windows matrix). It fails if any pinned checksum drifts or a download
  URL breaks.

## Rollout

Additive and backward-compatible: with no `[tools]` block and the tools already
on `$PATH`, behavior is unchanged except that a *missing* tool now triggers a
managed fetch (interactive) or an actionable error instead of a raw exec
failure. No migration required.
