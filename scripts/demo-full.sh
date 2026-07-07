#!/usr/bin/env bash
# Extended AgentOS demo.
#
# This script is intentionally honest: every displayed result comes from a real
# command. It does not print simulated output.

set -euo pipefail

WORKDIR="${AGENTOS_DEMO_DIR:-/tmp/agentos-demo}"

if ! command -v agentOS >/dev/null 2>&1; then
  echo "agentOS was not found in PATH."
  echo "Build or install it first:"
  echo "  cargo install --path crates/cli"
  exit 1
fi

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

echo "AgentOS extended demo"
echo "Demo directory: $WORKDIR"
mkdir -p "$WORKDIR"

run_step "Check environment" agentOS doctor
run_step "Create an agent manifest" agentOS init-manifest --name demo-researcher --with-permissions --force --output "$WORKDIR/agent.yaml"
run_step "Inspect generated manifest" cat "$WORKDIR/agent.yaml"
run_step "Create a TOML agent config" agentOS init-agent --name demo-agent --template research --force --output "$WORKDIR/agent.toml"
run_step "Inspect generated agent config" cat "$WORKDIR/agent.toml"
run_step "List templates" agentOS templates
run_step "List capabilities" agentOS capabilities
run_step "Show environment" agentOS env
run_step "Print quickstart workflow" agentOS quickstart
run_step "Scaffold a WASM plugin" agentOS init-plugin --name custom-counter --path "$WORKDIR" --force --readme
run_step "Search marketplace" agentOS marketplace search basic
run_step "Install local marketplace plugin scaffold" agentOS marketplace install --name demo-plugin --path "$WORKDIR"
run_step "List installed marketplace plugins" agentOS marketplace list

echo
echo "Extended demo complete."
echo "Run this manually to start the generated agent:"
echo "  agentOS run --agent $WORKDIR/agent.toml"
