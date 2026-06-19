<#
.SYNOPSIS
  Un-wedge a machine where devrig / k3d got stuck — orphaned per-slug clusters,
  zombie crash-looping serverlb proxies, and leaked `devrig-*-net` networks that
  `devrig cluster delete` never reaped.

.DESCRIPTION
  Removes ALL devrig/k3d Docker objects (clusters, containers, networks) and, by
  default, the devrig slug index + per-project state under %USERPROFILE%\.devrig
  (the managed `bin\` toolchain is preserved). Use -KeepState to keep the slug
  index and kubeconfigs. Supports -WhatIf to preview.

  Run from an elevated PowerShell. Safe to run repeatedly.

.NOTES
  Upgrade devrig to >= 0.38.x first (`devrig update`) — that release stabilizes
  the Windows slug and reaps orphaned clusters, which is what lets it stay clean
  after this sweep.
#>
[CmdletBinding(SupportsShouldProcess)]
param([switch]$KeepState)

function Get-DockerExe {
    $c = Get-Command docker -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    $p = 'C:\Program Files\Docker\Docker\resources\bin\docker.exe'
    if (Test-Path $p) { return $p }
    throw 'docker not found on PATH or in the default Docker Desktop location'
}
function Get-K3dExe {
    # Prefer devrig's vendored copy (~/.devrig/bin) — on a managed setup k3d may
    # not be on PATH at all. Fall back to PATH. Both .exe (Windows) and bare
    # (Unix) names are matched.
    foreach ($base in @($env:USERPROFILE, $HOME, "$env:SystemRoot\System32\config\systemprofile")) {
        if (-not $base) { continue }
        $dir = Join-Path $base '.devrig\bin'
        if (-not (Test-Path $dir)) { continue }
        $m = Get-ChildItem (Join-Path $dir 'k3d-*') -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending | Select-Object -First 1
        if ($m) { return $m.FullName }
    }
    $c = Get-Command k3d -ErrorAction SilentlyContinue
    if ($c) { return $c.Source }
    return $null
}

$docker = Get-DockerExe
$k3d = Get-K3dExe

Write-Host "    docker: $docker"
Write-Host "    k3d:    $(if ($k3d) { $k3d } else { '(not found — relying on docker to sweep k3d-* containers)' })"
Write-Host '==> stopping any running devrig'
Get-Process devrig -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

if ($k3d) {
    Write-Host '==> k3d cluster delete --all'
    if ($PSCmdlet.ShouldProcess('all k3d clusters', 'delete')) { & $k3d cluster delete --all 2>&1 | Out-Null }
}

Write-Host '==> force-removing leftover k3d containers (zombie serverlbs, half-deleted nodes)'
foreach ($id in (& $docker ps -aq --filter 'name=k3d-')) {
    if ($id -and $PSCmdlet.ShouldProcess($id, 'docker rm -f')) { & $docker rm -f $id 2>&1 | Out-Null }
}

Write-Host '==> removing leaked devrig-* / k3d-* networks'
foreach ($n in (& $docker network ls --format '{{.Name}}' | Where-Object { $_ -match '^(devrig-|k3d-)' })) {
    if ($PSCmdlet.ShouldProcess($n, 'docker network rm')) { & $docker network rm $n 2>&1 | Out-Null }
}
& $docker network prune -f 2>&1 | Out-Null

if (-not $KeepState) {
    Write-Host '==> clearing devrig slug index + per-project state (keeping bin\)'
    if ($PSCmdlet.ShouldProcess("$env:USERPROFILE\.devrig\slugs.json", 'remove')) {
        Remove-Item "$env:USERPROFILE\.devrig\slugs.json" -Force -ErrorAction SilentlyContinue
    }
    Get-ChildItem "$env:USERPROFILE\.devrig" -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -ne 'bin' } |
        ForEach-Object { if ($PSCmdlet.ShouldProcess($_.FullName, 'remove')) { Remove-Item $_.FullName -Recurse -Force -ErrorAction SilentlyContinue } }
}

Write-Host ''
Write-Host '==> remaining k3d containers / devrig networks (should be empty):'
& $docker ps -a --filter 'name=k3d-' --format '  {{.Names}} {{.Status}}'
& $docker network ls --format '{{.Name}}' | Where-Object { $_ -match '^(devrig-|k3d-)' } | ForEach-Object { "  $_" }
Write-Host '==> done.'
