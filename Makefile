.PHONY: all build check test lint clean release docs docker run repl ps audit coverage help \
        bench bench-bus bench-memory bench-vault install-hooks dashboard sdk-example \
        code-review wasm-plugin install install-windows demo demo-gif publish-pypi publish-npm \
        examples-all dev supervisor marketplace-list demo-full

CARGO = cargo

all: check test build

build:
	$(CARGO) build --workspace

release:
	$(CARGO) build --workspace --release

check:
	$(CARGO) check --workspace

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fix:
	$(CARGO) fmt --all
	$(CARGO) clippy --workspace --fix --allow-dirty

clean:
	$(CARGO) clean

docs:
	$(CARGO) doc --workspace --no-deps --document-private-items

docker:
	docker build -t agentos .

docker-run:
	docker run --rm -it agentos run --agent examples/simple_agent.toml

docker-compose-up:
	docker compose up --build

run:
	$(CARGO) run -p agentos-cli -- run --agent examples/simple_agent.toml

repl:
	$(CARGO) run -p agentos-cli -- repl

supervisor:
	$(CARGO) run -p agentos-cli -- supervisor

marketplace-list:
	$(CARGO) run -p agentos-cli -- marketplace list

ps:
	$(CARGO) run -p agentos-cli -- ps

audit:
	$(CARGO) audit

coverage:
	$(CARGO) llvm-cov --workspace --lcov --output-path lcov.info

# Benchmarks
bench:
	$(CARGO) bench --workspace

bench-bus:
	$(CARGO) bench -p agentos-benches --bench bus_benchmarks

bench-memory:
	$(CARGO) bench -p agentos-benches --bench memory_benchmarks

bench-vault:
	$(CARGO) bench -p agentos-benches --bench vault_benchmarks

test-integration:
	$(CARGO) test -p agentos-integration-tests

# Developer Experience
dev:
	$(CARGO) run -p agentos-cli -- dev --path .

install-hooks:
	cp .hooks/pre-commit .git/hooks/pre-commit
	cp .hooks/pre-push .git/hooks/pre-push
	chmod +x .git/hooks/pre-commit .git/hooks/pre-push
	@echo "Git hooks installed"

# Dashboard
dashboard-install:
	cd dashboard && npm install

dashboard-dev:
	cd dashboard && npm run dev

dashboard-build:
	cd dashboard && npm run build

# SDK Examples
sdk-example:
	$(CARGO) run -p agentos-sdk-examples

code-review:
	$(CARGO) run -p agentos-code-review-example

wasm-plugin:
	cd examples/wasm-plugin && cargo build --target wasm32-wasip1 --release

# Examples
examples-all: sdk-example code-review wasm-plugin
	@echo "All Rust examples run."

# Install
install:
	@echo "Run the install script directly:"
	@echo "  curl -fsSL https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/agentOS/main/install.sh | bash"
	@echo ""
	@echo "Or install from source:"
	@echo "  cargo install --path crates/cli"

install-windows:
	@echo "Run the install script in PowerShell:"
	@echo "  iwr -useb https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/agentOS/main/install.ps1 | iex"

# Demo
demo:
	bash scripts/demo.sh

demo-gif:
	bash scripts/demo.sh agentos-demo.gif

demo-full:
	bash scripts/demo-full.sh

# Publishing
publish-pypi:
	python3 scripts/publish_pypi.py

publish-pypi-test:
	python3 scripts/publish_pypi.py --test

publish-npm:
	bash scripts/publish_npm.sh

publish-npm-dryrun:
	bash scripts/publish_npm.sh --dry-run

help:
	@echo "AgentOS Development Commands"
	@echo "---------------------------"
	@echo "make build          - Build all crates"
	@echo "make release        - Build release artifacts"
	@echo "make check          - Check compilation"
	@echo "make test           - Run all tests"
	@echo "make lint           - Run fmt + clippy"
	@echo "make fix            - Auto-fix formatting and clippy"
	@echo "make clean          - Clean build artifacts"
	@echo "make docs           - Generate documentation"
	@echo "make docker         - Build Docker image"
	@echo "make run            - Run example agent"
	@echo "make repl           - Start interactive REPL"
	@echo "make bench          - Run all benchmarks"
	@echo "make coverage       - Generate test coverage report"
	@echo "make audit          - Run cargo audit"
	@echo "make install-hooks  - Install git hooks"
	@echo "make dashboard-dev  - Start dashboard dev server"
	@echo "make sdk-example    - Run Rust SDK example"
	@echo "make code-review    - Run multi-agent code review example"
	@echo "make wasm-plugin    - Build WASM plugin example"
	@echo "make examples-all   - Run all Rust examples"
	@echo "make demo           - Run terminal demo"
	@echo "make demo-gif       - Explain honest demo recording workflow"
	@echo "make publish-pypi   - Publish Python SDK to PyPI"
	@echo "make publish-npm    - Publish TypeScript SDK to npm"
	@echo "make install        - Print one-liner install instructions"
	@echo "make docker-compose-up  - Start experimental Docker stack"
	@echo "make dev            - Watch + auto-restart agent dev mode"
	@echo "make supervisor     - Real-time agent health dashboard"
	@echo "make marketplace-list  - List installed marketplace plugins"
