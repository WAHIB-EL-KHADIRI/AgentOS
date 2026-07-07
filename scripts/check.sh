#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

say() {
  printf '\n== %s ==\n' "$*"
}

run() {
  printf '$'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

resolve_command() {
  local primary="$1"
  local fallback="${2:-}"

  if command -v "$primary" >/dev/null 2>&1; then
    command -v "$primary"
    return 0
  fi

  if [ -n "$fallback" ] && command -v "$fallback" >/dev/null 2>&1; then
    command -v "$fallback"
    return 0
  fi

  return 1
}

check_security() {
  say "Security audit"
  if command -v cargo-audit >/dev/null 2>&1; then
    run cargo audit
  else
    echo "[skip] cargo-audit not installed; install with: cargo install cargo-audit"
  fi

  if command -v cargo-deny >/dev/null 2>&1; then
    run cargo deny check advisories bans licenses
  else
    echo "[skip] cargo-deny not installed; install with: cargo install cargo-deny"
  fi
}

check_tracked_artifacts() {
  say "Tracked generated artifact check"

  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "[skip] .git not found; tracked artifact check requires a git worktree."
    return 0
  fi

  local tracked
  tracked="$(
    git ls-files |
      grep -E '(^|/)(target|node_modules|dist|__pycache__)(/|$)|(^|/)[^/]+\.egg-info(/|$)|\.tsbuildinfo$' || true
  )"

  if [ -n "$tracked" ]; then
    echo "Generated artifacts are tracked by git:"
    printf '%s\n' "$tracked"
    echo
    echo "Remove these files from git history/index before publishing the repository."
    return 1
  fi

  echo "[ok] no generated artifacts are tracked"
}

run_npm_script() {
  local package_dir="$1"
  local script_name="$2"
  local label="$3"

  if [ ! -f "$package_dir/package.json" ]; then
    echo "[skip] $package_dir/package.json not found"
    return 0
  fi

  say "$label"
  (
    cd "$package_dir"
    local npm_bin

    if command -v npm.cmd >/dev/null 2>&1 && command -v cmd.exe >/dev/null 2>&1; then
      run cmd.exe /C npm.cmd run "$script_name"
    else
      npm_bin="$(resolve_command npm.cmd npm || true)"
      if [ -n "$npm_bin" ]; then
        run "$npm_bin" run "$script_name"
      else
        echo "npm was not found; cannot run $label."
        return 1
      fi
    fi
  )
}

check_dashboard() {
  run_npm_script "dashboard" "build" "Dashboard build"
  run_npm_script "dashboard" "lint" "Dashboard lint"
  run_npm_script "dashboard" "format:check" "Dashboard format check"
  run_npm_script "dashboard" "test" "Dashboard tests"
}

check_typescript_sdk() {
  run_npm_script "crates/sdk/typescript" "build" "TypeScript SDK build"
  run_npm_script "crates/sdk/typescript" "test" "TypeScript SDK tests"
}

check_python_sdk() {
  local package_dir="crates/sdk/python"
  local python_bin

  if [ ! -f "$package_dir/pyproject.toml" ]; then
    echo "[skip] $package_dir/pyproject.toml not found"
    return 0
  fi

  python_bin="$(resolve_command python3 python || true)"
  if [ -z "$python_bin" ]; then
    echo "[skip] python was not found; cannot check Python SDK."
    return 0
  fi

  if "$python_bin" -m pytest --version >/dev/null 2>&1; then
    say "Python SDK tests"
    (cd "$package_dir" && run "$python_bin" -m pytest)
  elif command -v cmd.exe >/dev/null 2>&1 && cmd.exe /C py -m pytest --version >/dev/null 2>&1; then
    say "Python SDK tests"
    (cd "$package_dir" && run cmd.exe /C py -m pytest)
  else
    echo "[skip] pytest was not found; cannot test Python SDK."
  fi

  if "$python_bin" -m ruff --version >/dev/null 2>&1; then
    say "Python SDK lint"
    (cd "$package_dir" && run "$python_bin" -m ruff check .)
  elif command -v cmd.exe >/dev/null 2>&1 && cmd.exe /C py -m ruff --version >/dev/null 2>&1; then
    say "Python SDK lint"
    (cd "$package_dir" && run cmd.exe /C py -m ruff check .)
  else
    echo "[skip] ruff was not found; cannot lint Python SDK."
  fi
}

check_security

check_tracked_artifacts

CARGO_BIN="$(resolve_command cargo cargo.exe || true)"
if [ -z "$CARGO_BIN" ]; then
  echo "cargo was not found. Install Rust or make cargo available in PATH."
  exit 1
fi

say "Rust format"
run "$CARGO_BIN" fmt --all --check

say "Rust workspace check"
run "$CARGO_BIN" check --workspace

say "Rust lint"
run "$CARGO_BIN" clippy --workspace --all-targets -- -D warnings

say "Rust workspace tests"
run "$CARGO_BIN" test --workspace

say "Rust benches check"
run "$CARGO_BIN" check --workspace --benches

say "Demo smoke check"
run bash scripts/demo.sh --check

check_dashboard
check_typescript_sdk
check_python_sdk

say "Done"
echo "[ok] AgentOS repository checks passed"
