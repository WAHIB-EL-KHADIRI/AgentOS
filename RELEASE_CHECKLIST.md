# Release Checklist

Use this checklist before publishing a GitHub release.

## Code

- [ ] `cargo fmt --all`
- [ ] `cargo check --workspace`
- [ ] workspace builds on CI
- [ ] public API changes are documented
- [ ] examples still match the current CLI and SDKs

## Documentation

- [ ] `README.md` reflects the current project state
- [ ] `ROADMAP.md` is updated
- [ ] `CHANGELOG.md` has a new entry
- [ ] `docs/architecture.md` matches the implementation
- [ ] security-sensitive changes are reflected in `docs/security-model.md`

## Community

- [ ] good first issues are available
- [ ] open questions are updated
- [ ] contribution paths are clear
- [ ] release notes credit contributors

## Release Notes Template

```text
## AgentOS vX.Y.Z

### Highlights

- 

### Added

- 

### Changed

- 

### Fixed

- 

### Contributors

- 
```

