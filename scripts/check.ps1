# AgentOS repository check script for Windows PowerShell 5.1+
# Equivalent to scripts/check.sh for Windows environments.

$ErrorActionPreference = "Stop"
$ROOT_DIR = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location -LiteralPath $ROOT_DIR

function Say($msg) { Write-Host "`n== $msg ==" -ForegroundColor Cyan }

function Run {
    Write-Host "$" -NoNewline
    foreach ($a in $args) { Write-Host " $a" -NoNewline }
    Write-Host ""
    & $args[0] $args[1..($args.Length-1)]
    if ($LASTEXITCODE -ne 0) { throw "Command failed with exit code $LASTEXITCODE" }
}

function Resolve-Cmd {
    param([string]$Primary, [string]$Fallback)
    if (Get-Command $Primary -ErrorAction SilentlyContinue) { return $Primary }
    if ($Fallback -and (Get-Command $Fallback -ErrorAction SilentlyContinue)) { return $Fallback }
    return $null
}

function Check-TrackedArtifacts {
    Say "Tracked generated artifact check"
    $inGit = $false
    try { git rev-parse --is-inside-work-tree 2>$null | Out-Null; $inGit = $true } catch {}
    if (-not $inGit) { Write-Host "[skip] .git not found"; return }

    $tracked = git ls-files | Select-String '(^|/)(target|node_modules|dist|__pycache__)(/|$)|(^|/)[^/]+\.egg-info(/|$)|\.tsbuildinfo$'
    if ($tracked) {
        Write-Host "Generated artifacts are tracked by git:" -ForegroundColor Red
        $tracked | ForEach-Object { Write-Host $_ }
        Write-Host "Remove these files from git history/index before publishing."
        throw "Generated artifacts tracked in git"
    }
    Write-Host "[ok] no generated artifacts are tracked" -ForegroundColor Green
}

Say "Rust format"
Run cargo fmt --all --check

Say "Rust workspace check"
Run cargo check --workspace

Say "Rust lint"
Run cargo clippy --workspace --all-targets -- -D warnings

Say "Rust workspace tests"
Run cargo test --workspace

Say "Rust benches check"
Run cargo check --workspace --benches

# Run dashboard checks if available
$npmCmd = Resolve-Cmd -Primary "npm.cmd" -Fallback "npm"
if ($npmCmd -and (Test-Path "dashboard/package.json")) {
    Say "Dashboard build"
    Push-Location dashboard
    try {
        if (-not (Test-Path "node_modules")) { Run cmd.exe /C $npmCmd ci }
        Run cmd.exe /C $npmCmd run build
    } finally { Pop-Location }
} else {
    Write-Host "[skip] dashboard checks (npm not found or no package.json)"
}

Say "Done"
Write-Host "[ok] AgentOS repository checks passed" -ForegroundColor Green
