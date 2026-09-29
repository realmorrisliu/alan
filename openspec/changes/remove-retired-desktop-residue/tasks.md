## 1. Implementation

- [x] Inventory retirement decisions and live consumers.
- [x] Remove Ghostty, desktop IPC, bundled Skill and unused channel fields.
- [x] Remove stale signing template and historical absence checks.
- [x] Retain current Host-source security checks and remove redundant gate wiring.
- [x] Record retained platform, migration, parked and historical boundaries.
- [x] Address review: remove remaining README examples and retire persisted desktop Skill through Package Service.

## 2. Verification

- [x] Run focused current-behavior tests and repository quality gate.
- [x] Validate OpenSpec change and inspect final diff.

## 3. Delivery

- [ ] PR review and merge.
- [ ] Sync canonical deltas and the quality-gate purpose after merge; assess archive readiness.

## Verification results (2026-09-29)

- `just quality`: passed, including workspace Clippy/rustdoc, architecture and
  source-size gates, Host-source boundaries, OpenSpec current-surface checks,
  CLI build and standalone install/upgrade/uninstall/release validation.
- `cargo test --locked -p alan --bin alan`: 25 passed.
- `cargo test --locked -p alan-agent-engine --lib skills::`: 131 passed.
- `bash scripts/test-install-channel-descriptor.sh`: passed.
- `openspec validate remove-retired-desktop-residue --strict`: passed.
- `git diff --check`: passed. Ghostty gitlink, checkout and local submodule
  registration are absent; retired gate scripts have no live CI/Just callers.

PR #1025 initial Codex review found a persisted-package retirement gap and stale
README examples. Both are addressed; current-head re-review and CI are pending.
