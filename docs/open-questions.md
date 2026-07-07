# Open Questions

These questions are intentionally public. They give contributors concrete
problems to discuss and help shape the project.

## Runtime

- What should be the default restart policy?
- Should agents run as tasks, processes, containers, or all three?
- How should the supervisor detect a stuck agent?
- How should heartbeat failures be represented in lifecycle events?

## Trace And Replay

- What is the minimal checkpoint data needed for useful replay?
- Which parts of replay must be deterministic?
- How should AgentOS represent external tool results?
- Should trace branches be stored as full copies or structural diffs?

## Bus

- Should message topics be free-form strings or typed enums?
- How should backpressure work?
- Should the bus support broadcast, direct messages, and request-response?
- What metadata is required for debugging and audit trails?

## Security

- How strict should default permissions be?
- Should tool permissions be declared in agent config?
- How should secret access be audited?
- What should be encrypted at rest first?

## Dashboard

- What is the most useful first screen: live agents or trace timeline?
- How should branch comparison be visualized?
- Should the dashboard connect through HTTP, WebSocket, or gRPC-web?

## SDKs

- What should the Python SDK make easy first?
- What should the TypeScript SDK make easy first?
- Should SDKs expose low-level bus calls, high-level agent helpers, or both?

