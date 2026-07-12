## Context

After the retired host-service removal, current code still accepts several retired input
shapes: an unversioned LLMFS request, single-threshold compaction configuration,
old governance names, and an input operation with an alias or implicit routing
mode. The Apple client also retains three deprecated palette aliases, and the
workspace manifest declares unused dependencies and an unused Axum WebSocket
feature.

These are not one migration family and do not justify a compatibility layer.
They are grouped because each is a small hard-cut deletion with the same proof:
the canonical form succeeds, the retired form fails, and no fallback remains.
`clean-canonical-spec-debt` is a prerequisite. This change may proceed
independently of `remove-legacy-macos-persistence`; both complete before
`finish-namespace-native-engine-boundary` begins.

## Goals / Non-Goals

**Goals:**

- Make every affected parser accept one current shape.
- Delete aliases and fields rather than deprecating them again.
- Preserve actionable errors through ordinary deserialization and validation.
- Remove dependency declarations and features not reachable from production or
  tests.
- Add negative tests that prove silent compatibility does not return.

**Non-Goals:**

- Change provider-native wire fields derived from canonical reasoning effort.
- Change compaction algorithms or threshold semantics.
- Redesign governance, request routing, or the AgentFS layout.
- Remove Axum from LLM HTTP tests.
- Touch macOS persisted formats, installer cleanup, or privileged legacy state.

## Decisions

### 1. Retired inputs fail at the owning parser

No pre-parser, migration pass, warning-only path, or alias remains. Existing
`deny_unknown_fields` behavior in agent configuration will reject removed TOML
keys. Serde enum and operation parsing will accept only canonical spellings and
required fields. LLMFS will decode only its current versioned request DTO.

Alternative considered: accept and warn for one release. Rejected because Alan
is in early development and the explicit goal is to stop treating these shapes
as supported contracts.

### 2. LLMFS has one request document version on the live path

The `RequestDoc` legacy branch and `LegacyRequestDoc` are deleted. The server
accepts only the current versioned document and maps it to `GenerationRequest`.
Malformed, unversioned, or unknown-version documents fail at Generation commit
before provider dispatch. Existing tests and fixtures using `{ "user": ... }`
will be rewritten to the v1 shape except for targeted rejection fixtures.

Alternative considered: retain an enum to prepare for v2. Rejected; a future v2
can introduce explicit version dispatch when v2 exists, without preserving v0.

### 3. Configuration names represent current semantics only

`compaction_soft_trigger_ratio` and `compaction_hard_trigger_ratio` are the only
threshold keys. `model_reasoning_effort` is the only public reasoning control.
`compaction_trigger_ratio` and `thinking_budget_tokens` are removed from types,
defaults, docs, fixtures, and serialization tests. Provider adapters may still
derive a provider-native numeric thinking budget from canonical effort; that is
wire projection, not a public compatibility field.

### 4. Governance and input routing are explicit

`GovernanceProfile` decodes only `autonomous`. `Op::Input` decodes only
`type: "input"` and requires a `mode` field. The current mode values remain
unchanged. Missing mode and `type: "steer"` are errors, so routing intent cannot
be inferred from a retired default.

Alternative considered: keep `autonomous` plus `conservative` as labels for the
same posture. Rejected because multiple names imply multiple selectable modes
and contradict the single-posture contract.

### 5. Cosmetic and dependency aliases are deleted in the same bounded pass

Remaining `ShellPalette.rootBacking`, `canvas`, and `window` call sites move to
their canonical `ShellPaper` tokens, then the aliases are deleted. Workspace
dependencies are removed only after `cargo metadata`, source search, and the
full feature build prove there is no owner. Axum remains a dev dependency for
HTTP tests but loses the `ws` feature and its transitive WebSocket stack.

## Risks / Trade-offs

- [Local config stops loading] → Fail with the normal unknown-field error and
  name replacements in release notes; do not auto-edit user files.
- [External operation producer omits mode] → Add protocol rejection fixtures
  and update every repository producer in the same change.
- [LLMFS tests accidentally keep exercising v0] → Convert positive fixtures to
  v1 and retain one explicit v0 rejection test.
- [Dependency declaration is indirectly required] → Verify workspace builds,
  all targets, all features, and focused macOS contract tests before removal.
- [Provider-native numeric budget is mistaken for public compatibility] → Keep
  tests separating canonical effort input from adapter-owned wire projection.

## Migration Plan

1. Convert repository-owned LLMFS, governance, and input-operation producers to
   canonical forms.
2. Delete legacy parser variants, aliases, and config fields.
3. Move palette call sites and delete deprecated tokens.
4. Remove unused workspace dependencies and the Axum WebSocket feature.
5. Add focused rejection tests, then run formatting, Clippy, workspace tests,
   Apple contract tests, and dependency inspection.
6. Land independently of the macOS persistence cleanup; both must be complete
   before the engine-boundary change starts.

Rollback is a normal revert of this change. No data migration is written, so
rollback does not need to reverse generated state.

## Open Questions

None.
