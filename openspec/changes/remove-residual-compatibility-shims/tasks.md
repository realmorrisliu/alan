## 1. Prerequisite And Inventory

- [x] 1.1 Start from main after `clean-canonical-spec-debt` is merged and verify its OpenSpec/current-surface guard is green.
- [x] 1.2 Inventory repository producers, positive fixtures, and rejection fixtures for each retired LLMFS, config, governance, input-operation, palette, and dependency surface before deleting types.

## 2. LLMFS Request Hard Cut

- [x] 2.1 Convert every repository-owned positive LLMFS request fixture and writer to the current explicitly versioned request DTO.
- [x] 2.2 Delete `LegacyRequestDoc`, the legacy `RequestDoc` branch, and unversioned `{system,user}` mapping from `crates/llmfs`.
- [x] 2.3 Add commit-time tests proving the current version succeeds while unversioned, missing-version, malformed, and unknown-version documents fail before provider dispatch.

## 3. Configuration Hard Cut

- [x] 3.1 Remove `compaction_trigger_ratio` from config types, defaulting/resolution code, examples, serialization, and positive fixtures while retaining soft/hard threshold validation.
- [x] 3.2 Remove `thinking_budget_tokens` from all agent-facing config/protocol/client types and fixtures while retaining adapter-owned derivation from canonical reasoning effort where a provider wire format requires it.
- [x] 3.3 Add focused unknown-field tests proving neither retired key is migrated, ignored, or copied into a canonical field.

## 4. Governance And Input Routing

- [x] 4.1 Remove `auto_approve`, `auto-approve`, `autoapprove`, and `conservative` aliases from `GovernanceProfile`; keep only serialized `autonomous`.
- [x] 4.2 Update all repository-owned governance producers and fixtures to `autonomous`, then add rejection tests for every retired alias and malformed/wrong-typed values.
- [x] 4.3 Remove the `type: "steer"` alias and the default for `Op::Input.mode`; update all producers to emit `type: "input"` with an explicit supported mode.
- [x] 4.4 Add protocol tests proving canonical explicit input succeeds and missing mode or retired `steer` type fails without inferred routing.

## 5. Palette And Dependency Cleanup

- [x] 5.1 Move remaining Apple call sites from `ShellPalette.rootBacking`, `canvas`, and `window` to their canonical `ShellPaper` tokens, then delete the deprecated aliases.
- [x] 5.2 Use source search and `cargo metadata` to reconfirm `tokio-stream`, `bytes`, `tracing-appender`, `tower`, `tower-http`, `config`, and the root `terminal_size` declarations are unused before removing them.
- [x] 5.3 Remove the unused Axum `ws` feature from the LLM test dev dependency while retaining Axum HTTP test-server coverage, then confirm WebSocket-only transitive crates leave the resolved graph when no other owner requires them.

## 6. Verification And Delivery

- [x] 6.1 Run focused `alan-llmfs`, `alan-agent-protocol`, `alan-agent-engine`, and `alan-llm` tests, including all new retired-shape rejection cases.
- [x] 6.2 Run `cargo fmt --all --check`, workspace Clippy with all targets/features and warnings denied, `cargo test --workspace`, `just apple-shell-focused-tests`, and `git diff --check`.
- [x] 6.3 Run `cargo metadata` and repository searches to prove removed fields, aliases, palette tokens, dependency declarations, and Axum WebSocket features have no current owner.
- [ ] 6.4 Update affected canonical Purpose text during spec sync so it describes only current reasoning, compaction, governance, and input contracts rather than removed compatibility.
- [ ] 6.5 Open a narrowly scoped PR and keep the current HEAD under Codex review until every thread is resolved, required CI is green, and a delayed refresh shows no new findings before merge.
- [ ] 6.6 After merge, sync all five capability deltas into canonical specs and mark the change archive-ready only when main rejects every retired form and `remove-legacy-macos-persistence` is either merged or independently green.
