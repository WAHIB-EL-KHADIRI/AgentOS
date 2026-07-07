# AgentOS Demo Assets

This directory is reserved for real demo recordings and screenshots. It exists
to keep future public assets organized without adding fake screenshots.

Recommended structure:

```text
assets/demo/
  recordings/   asciinema casts or short terminal recordings
  screenshots/  real screenshots captured from the CLI or dashboard
```

Guidelines:

- Use `bash scripts/demo.sh` as the source of truth.
- Do not add fake screenshots, edited terminal output, or mock traces.
- Prefer small assets such as asciinema casts or optimized GIFs.
- Keep large generated files out of the repository unless they are needed for a
  release or README update.
- Name assets by the real flow they show, for example
  `run-ps-logs-trace-replay.cast`.

Suggested recording command:

```bash
asciinema rec assets/demo/recordings/run-ps-logs-trace-replay.cast -c 'bash scripts/demo.sh'
```
