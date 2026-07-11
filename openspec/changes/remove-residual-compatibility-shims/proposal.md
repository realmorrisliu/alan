## Why

The retired host-service clean break left a smaller set of unrelated compatibility readers,
aliases, deprecated fields, and manifest declarations that still accept or carry
retired shapes. Alan is early enough to remove them now; retaining them would make
old inputs part of the supported surface and obscure the one canonical form.

## What Changes

- **BREAKING**: require the versioned LLMFS v1 request document and reject the
  unversioned `{system,user}` skeleton shape without migration or fallback.
- **BREAKING**: remove `compaction_trigger_ratio` and
  `thinking_budget_tokens`; configuration using either field fails normal
  unknown-field validation and must use the dual compaction thresholds and
  `model_reasoning_effort`.
- **BREAKING**: accept only the `autonomous` governance profile spelling; remove
  `auto_approve`, `auto-approve`, `autoapprove`, and `conservative` aliases.
- **BREAKING**: require `type=input` with an explicit routing `mode`; remove the
  legacy `type=steer` alias and the missing-mode default.
- Remove the deprecated `ShellPalette.rootBacking`, `.canvas`, and `.window`
  aliases after migrating their remaining call sites to `ShellPaper`.
- Remove unused workspace dependency declarations and the unused Axum WebSocket
  feature while retaining Axum as an LLM test-only HTTP server dependency.
- Add regression checks proving retired spellings and request shapes fail rather
  than silently resolving, migrating, or being ignored.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `llm-file-server`: make the versioned request DTO the only accepted Generation
  request document.
- `provider-request-controls`: remove all thinking-budget and single-threshold
  request-control compatibility.
- `runtime-memory-contract`: make dual soft/hard compaction thresholds the only
  accepted compaction configuration.
- `auto-approve-policy`: remove legacy governance profile spellings and retain
  only the canonical `autonomous` value.
- `agent-file-layout-contract`: require canonical, explicit input-routing
  documents rather than accepting legacy operation aliases or implicit modes.

## Impact

- `crates/llmfs`, `crates/agent-engine/src/config.rs`, and
  `crates/agent-protocol/src/op.rs`, plus their tests and fixtures.
- `clients/apple/alan-macos/Support/ShellDesignTokens.swift` and remaining alias
  call sites.
- Root Cargo dependency declarations and `crates/llm` dev-dependency features.
- Existing local configuration or external callers using removed spellings must
  be updated manually; this change creates no compatibility reader or migrator.
- macOS persistence formats, installer migration, and privileged legacy-state
  cleanup are owned by `remove-legacy-macos-persistence`, not this change.
- This change begins after `clean-canonical-spec-debt` lands and may proceed
  independently of `remove-legacy-macos-persistence`.
