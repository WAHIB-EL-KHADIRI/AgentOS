#!/usr/bin/env bash
# AgentOS killer demo.
#
# Goal: show the real local runtime workflow in under two minutes:
# run -> ps -> logs -> trace -> replay.
#
# This script does not print fabricated command output. Every command result is
# produced by the current AgentOS CLI.

set -euo pipefail

AGENTOS_BIN="${AGENTOS_BIN:-agentOS}"
AGENT_CONFIG="${AGENTOS_DEMO_AGENT:-examples/simple_agent.toml}"
AGENT_ID="${AGENTOS_DEMO_AGENT_ID:-agent_simple_agent}"
DEMO_DIR="${AGENTOS_DEMO_DIR:-.agentos/demo}"
HTTP_PORT="${AGENTOS_DEMO_HTTP_PORT:-18080}"
GRPC_PORT="${AGENTOS_DEMO_GRPC_PORT:-15051}"
RUN_SECONDS="${AGENTOS_DEMO_RUN_SECONDS:-5}"
RUNTIME_CONFIG="$DEMO_DIR/agentos.demo.toml"
MODE="${1:-run}"

usage() {
  echo "Usage:"
  echo "  bash scripts/demo.sh"
  echo "  bash scripts/demo.sh --check"
}

require_agentos() {
  if ! command -v "$AGENTOS_BIN" >/dev/null 2>&1; then
    if [ "$AGENTOS_BIN" = "agentOS" ]; then
      for candidate in ./target/debug/agentOS ./target/debug/agentOS.exe; do
        if [ -f "$candidate" ]; then
          AGENTOS_BIN="$candidate"
          return 0
        fi
      done
    fi

    echo "agentOS was not found in PATH."
    echo "Build or install it first:"
    echo "  cargo build --workspace"
    echo "  AGENTOS_BIN=./target/debug/agentOS bash scripts/demo.sh --check"
    echo "  cargo install --path crates/cli"
    exit 1
  fi
}

require_demo_files() {
  if [ ! -f "$AGENT_CONFIG" ]; then
    echo "Agent config not found: $AGENT_CONFIG"
    echo "Run this script from the AgentOS repository root."
    exit 1
  fi

  if [ ! -f "docs/demo.md" ]; then
    echo "Demo documentation not found: docs/demo.md"
    exit 1
  fi
}

demo_smoke_check() {
  echo "AgentOS demo smoke check"

  require_demo_files
  require_agentos

  bash -n "$0"

  local help_output
  help_output="$("$AGENTOS_BIN" --help)"
  for command in run ps logs trace replay init-runtime inspect-config; do
    if ! printf '%s\n' "$help_output" | grep -Eq "(^|[[:space:]])${command}([[:space:]]|$)"; then
      echo "Missing CLI command in help output: $command"
      exit 1
    fi
  done

  "$AGENTOS_BIN" run --help >/dev/null
  "$AGENTOS_BIN" ps --help >/dev/null
  "$AGENTOS_BIN" logs --help >/dev/null
  "$AGENTOS_BIN" trace --help >/dev/null
  "$AGENTOS_BIN" replay --help >/dev/null
  "$AGENTOS_BIN" inspect-config --agent "$AGENT_CONFIG" >/dev/null

  if ! grep -q "run -> ps -> logs -> trace -> replay" docs/demo.md; then
    echo "docs/demo.md does not document the official demo flow."
    exit 1
  fi

  if grep -nE '2>/dev/null[[:space:]]*\|\||\|\|[[:space:]]*echo|No agents running|System healthy|tool: web_search|llm response' "$0" | grep -v 'grep -nE'; then
    echo "Demo script contains fallback or fabricated-output patterns."
    exit 1
  fi

  echo "[ok] demo script syntax is valid"
  echo "[ok] demo files are present"
  echo "[ok] core CLI commands are available"
  echo "[ok] demo docs describe the official flow"
  echo "[ok] no known fake-output fallback patterns found"
}

if [ "$MODE" = "--help" ] || [ "$MODE" = "-h" ]; then
  usage
  exit 0
fi

if [ "$MODE" = "--check" ] || { [ "${CI:-}" = "1" ] && [ "$MODE" = "check" ]; }; then
  demo_smoke_check
  exit 0
fi

if [ "$MODE" != "run" ]; then
  echo "This script does not record GIF output directly."
  echo "Record it with a terminal recorder if needed:"
  echo "  asciinema rec assets/demo/agentos-demo.cast -c 'bash scripts/demo.sh'"
  echo
fi

require_agentos
require_demo_files

run_step() {
  local title="$1"
  shift

  echo
  echo "== $title =="
  printf '$'
  printf ' %q' "$@"
  echo
  "$@"
}

run_agent_for_demo() {
  echo
  echo "== Run a real agent for ${RUN_SECONDS}s, then interrupt the demo process =="
  printf '$ %q run --agent %q --config %q\n' "$AGENTOS_BIN" "$AGENT_CONFIG" "$RUNTIME_CONFIG"

  set +e
  "$AGENTOS_BIN" run --agent "$AGENT_CONFIG" --config "$RUNTIME_CONFIG" &
  local pid=$!
  local interrupted=0

  sleep "$RUN_SECONDS"
  if kill -0 "$pid" >/dev/null 2>&1; then
    echo
    echo "[demo] interrupting agentOS run after ${RUN_SECONDS}s"
    interrupted=1
    kill -INT "$pid" >/dev/null 2>&1 || kill "$pid" >/dev/null 2>&1
  fi

  wait "$pid"
  local status=$?
  set -e

  if [ "$status" -ne 0 ] && [ "$interrupted" -eq 1 ]; then
    echo "[demo] agentOS run exited with status $status after the intentional demo interrupt."
  elif [ "$status" -ne 0 ]; then
    echo "[demo] agentOS run failed before the demo interrupt with status $status."
    exit "$status"
  fi
}

extract_checkpoint() {
  "$AGENTOS_BIN" trace --id "$AGENT_ID" \
    | grep -Eo '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}' \
    | tail -n 1
}

dashboard_hint() {
  echo
  echo "== Dashboard status =="
  if command -v curl >/dev/null 2>&1 && curl -fsS --max-time 2 "http://127.0.0.1:5173" >/dev/null 2>&1; then
    echo "Dashboard dev server detected at http://127.0.0.1:5173"
    echo "Open it to see the real connection indicator. It will show Live only if the SSE endpoint is reachable."
  else
    echo "Dashboard dev server was not detected."
    echo "Optional manual step:"
    echo "  cd dashboard && npm run dev"
    echo "  open http://127.0.0.1:5173"
  fi
}

echo "AgentOS killer demo"
echo "Real commands only: run -> ps -> logs -> trace -> replay"
echo
echo "Notes:"
echo "- Restart/recovery is not shown here; treat it as future/experimental demo work."
echo "- The run command is interrupted after ${RUN_SECONDS}s so the demo can finish quickly."

mkdir -p "$DEMO_DIR"

run_step "Prepare an isolated runtime config" \
  "$AGENTOS_BIN" init-runtime --output "$RUNTIME_CONFIG" --http-port "$HTTP_PORT" --grpc-port "$GRPC_PORT" --force

run_step "Inspect the demo agent config" \
  "$AGENTOS_BIN" inspect-config --agent "$AGENT_CONFIG"

run_agent_for_demo

run_step "List agents from real local state" \
  "$AGENTOS_BIN" ps --all

run_step "Show logs for the demo agent" \
  "$AGENTOS_BIN" logs --id "$AGENT_ID"

run_step "Show trace checkpoints for the demo agent" \
  "$AGENTOS_BIN" trace --id "$AGENT_ID"

CHECKPOINT="$(extract_checkpoint)"
if [ -z "$CHECKPOINT" ]; then
  echo "Could not find a checkpoint for $AGENT_ID."
  echo "Run: $AGENTOS_BIN trace --id $AGENT_ID"
  exit 1
fi

run_step "Replay the latest real checkpoint" \
  "$AGENTOS_BIN" replay --checkpoint "$CHECKPOINT"

dashboard_hint

echo
echo "Demo complete."
