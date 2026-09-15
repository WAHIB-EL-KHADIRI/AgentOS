#!/usr/bin/env pwsh
# AgentOS one-line install (Windows)
# Usage: iwr -useb https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/AgentOS/main/install.ps1 | iex

$Repo = "WAHIB-EL-KHADIRI/AgentOS"
$BinDir = if ($env:AGENTOS_BIN) { $env:AGENTOS_BIN } else { "$HOME\.agentos\bin" }
$Version = if ($env:AGENTOS_VERSION) { $env:AGENTOS_VERSION } else { "latest" }

function Write-Step($msg) { Write-Host "[AgentOS] $msg" -ForegroundColor Green }
function Stop-AgentOSInstall($msg) { Write-Host "[AgentOS] $msg" -ForegroundColor Red; exit 1 }

$Target = "x86_64-pc-windows-msvc"

# Sanitize version tag to prevent path injection
$Tag = if ($Version -eq "latest") { "latest" } else { $Version -replace '[/\0]', '_' }

if ($Version -eq "latest") {
  # /releases/latest excludes pre-releases, and every AgentOS release so far
  # is one -- so that endpoint 404s on this repository. Read the release list
  # instead and take the newest entry.
  $ApiUrl = "https://api.github.com/repos/$Repo/releases?per_page=1"
  try {
    $Release = Invoke-RestMethod -Uri $ApiUrl -ErrorAction Stop | Select-Object -First 1
    $Tag = $Release.tag_name
    if (-not $Tag) { throw "the release list came back empty" }
  } catch {
    Stop-AgentOSInstall "Could not resolve the latest release from the GitHub API. Build from source with: cargo install --path crates/cli"
  }
} else {
  $Tag = $Version
}

$Archive = "agentOS-${Tag}-${Target}.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Tag/$Archive"
$TempZip = "$env:TEMP\$Archive"
$ExtractDir = "$env:TEMP\agentOS-install"

Write-Step "Downloading AgentOS $Tag for $Target"

Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempZip -UseBasicParsing -TimeoutSec 120 -ErrorAction Stop

if (Test-Path $ExtractDir) { Remove-Item -Recurse -Force $ExtractDir }
New-Item -ItemType Directory -Path $ExtractDir -Force | Out-Null
Expand-Archive -Path $TempZip -DestinationPath $ExtractDir -Force

New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
Copy-Item "$ExtractDir\agentOS.exe" "$BinDir\agentOS.exe" -Force
Remove-Item -Recurse -Force $ExtractDir, $TempZip

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$BinDir*") {
  $NewPath = "$UserPath;$BinDir"
  [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
  $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + $NewPath
}

Write-Step "AgentOS $Tag installed to $BinDir"
Write-Host ""
Write-Host "Quick start:" -ForegroundColor Cyan
Write-Host "  agentOS quickstart" -ForegroundColor White
Write-Host "  agentOS run --help" -ForegroundColor White
