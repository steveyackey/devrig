# devrig installer for Windows — downloads the latest release archive,
# verifies its SHA256, and installs devrig.exe (dashboard included).
#
#   powershell -ExecutionPolicy Bypass -c "irm https://github.com/steveyackey/devrig/releases/latest/download/install.ps1 | iex"
#
# Env overrides:
#   DEVRIG_INSTALL_DIR   target dir (default: %LOCALAPPDATA%\devrig\bin)
#   DEVRIG_VERSION       version tag to install (default: latest)
$ErrorActionPreference = "Stop"

$Repo = "steveyackey/devrig"
$InstallDir = if ($env:DEVRIG_INSTALL_DIR) { $env:DEVRIG_INSTALL_DIR } else { "$env:LOCALAPPDATA\devrig\bin" }

# Capture any pre-existing devrig before we install, to migrate off an old
# cargo-dist (Rust) install (binary + devrig-update.exe sidecar + receipt).
$preDevrig = (Get-Command devrig -ErrorAction SilentlyContinue).Source

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
	"ARM64" { "arm64" }
	default { "x86_64" }
}

$version = $env:DEVRIG_VERSION
if (-not $version) {
	$rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "devrig-install" }
	$version = $rel.tag_name
}
if (-not $version) { throw "could not determine latest version" }
$ver = $version.TrimStart("v")

$archive = "devrig_${ver}_windows_${arch}.zip"
$base = "https://github.com/$Repo/releases/download/$version"
$tmp = Join-Path $env:TEMP ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
	Write-Host "Downloading $archive ..."
	Invoke-WebRequest -Uri "$base/$archive" -OutFile "$tmp\$archive"
	Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile "$tmp\SHA256SUMS"

	Write-Host "Verifying checksum ..."
	$expected = (Get-Content "$tmp\SHA256SUMS" |
		Where-Object { $_ -match [regex]::Escape($archive) } |
		Select-Object -First 1).Split(" ")[0].ToLower()
	$actual = (Get-FileHash "$tmp\$archive" -Algorithm SHA256).Hash.ToLower()
	if (-not $expected) { throw "no checksum entry for $archive" }
	if ($actual -ne $expected) { throw "checksum mismatch for $archive" }

	Expand-Archive -Path "$tmp\$archive" -DestinationPath $tmp -Force
	New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
	Copy-Item -Path "$tmp\devrig.exe" -Destination "$InstallDir\devrig.exe" -Force

	$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
	if ($userPath -notlike "*$InstallDir*") {
		[Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
		Write-Host "Added $InstallDir to your user PATH (restart your shell to pick it up)."
	}
	Write-Host "Installed devrig $ver to $InstallDir\devrig.exe"

	# --- migrate off an old cargo-dist (Rust) install ---
	$newBin = Join-Path $InstallDir "devrig.exe"
	$receipt = Join-Path $env:LOCALAPPDATA "devrig\devrig-receipt.json"
	$migrated = $false
	if ($preDevrig -and ($preDevrig -ne $newBin)) {
		$oldDir = Split-Path $preDevrig -Parent
		$sidecar = Join-Path $oldDir "devrig-update.exe"
		# Only remove something positively identified as the cargo-dist install.
		if ((Test-Path $sidecar) -or (Test-Path $receipt)) {
			Remove-Item -Force $preDevrig -ErrorAction SilentlyContinue
			Remove-Item -Force $sidecar -ErrorAction SilentlyContinue
			Write-Host "Removed old install at $preDevrig (and its devrig-update sidecar)"
			$migrated = $true
		}
	}
	$localSidecar = Join-Path $InstallDir "devrig-update.exe"
	if (Test-Path $localSidecar) { Remove-Item -Force $localSidecar -ErrorAction SilentlyContinue; $migrated = $true }
	if (Test-Path $receipt) {
		Remove-Item -Force $receipt -ErrorAction SilentlyContinue
		Remove-Item -Force (Split-Path $receipt -Parent) -ErrorAction SilentlyContinue
		$migrated = $true
	}
	if ($migrated) { Write-Host "Migrated from the previous (Rust) devrig install." }

	Write-Host "Run 'devrig update' to upgrade in place later."
}
finally {
	Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
