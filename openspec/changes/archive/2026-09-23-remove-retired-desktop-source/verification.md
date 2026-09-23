# Local verification

Base: main 04753a2f. Implementation branch: codex/remove-retired-desktop-source.
These are local results, not PR/CI/merge evidence.

- Removed 286 tracked files under clients/apple and shell-core/FFI, plus the
  desktop design-language guide. Existing ignored artifacts and the independent
  Ghostty checkout are untouched; Git retains the removed source.
- Cargo.lock changes only remove the two retired workspace packages.
- No runtime changes under alan, os-host, agent-engine, auth, llm, knowledge
  or kernel. Existing q Skill distribution and Rust security owners remain.
- cargo check --workspace --all-targets --offline: passed.
- cargo test --workspace --locked --quiet: passed; opt-in live tests remain ignored.
- scripts/check-quality.sh: passed, including Rust lint/dependency gates and
  CLI/Host checks.
- Desktop absence guard: clean tree passed; a temporary source file under
  crates/shell-core was correctly rejected, then removed and the check passed.
- openspec validate --all --strict: 86 passed, 0 failed.
- git diff --check: passed.

- scripts/test-standalone-cli-distribution.sh: passed after fresh release
  builds; stable/dev installation, version output, refusal to overwrite an
  unrelated file and archive content checks passed in an isolated directory.

## Pull request delivery

- PR [#926](https://github.com/realmorrisliu/alan/pull/926) merged as
  `1586e3899fa63fadba3d4db96878b2886289f082` on 2026-09-20.
- Final Codex review of `ef07ed2bbc6abf3915e293e7cd82c84c2bf20862` found no
  major issues. All four review threads were resolved.
- All 16 current-head CI checks passed before merge.

Canonical sync and archive are tracked by task 2.3.

## Canonical specification sync

- Removed all 21 desktop-only canonical capability files. Each removal delta's
  requirement names match the corresponding main-branch specification.
- Synced the added requirements into `documentation-governance` and
  `repository-quality-gate`, preserving the Rust CLI/Host security owners.
- Updated active OpenSpec references, including the cancelled UPDF desktop
  preview task; that parked delta remains unchecked and is not synchronized.
- `openspec validate --all --strict`: 65 passed, 0 failed. Existing long-
  requirement advisories remain informational.
- `git diff --check`: passed. No active non-archived change references the
  retired desktop capability names outside this removal record.

The change is ready to archive with specs already synchronized.

## Review correction

PR #926 identified a fresh-checkout regression: the removed clients scan root
made ripgrep return status 2, which conditional callers treated as no matches.
Removed that root and made the shared search function fail on search errors
for both rg and git grep. The quality gate now runs an isolated regression
covering all three forbidden-source scans without clients, plus missing-root
errors with and without a forbidden match. No runtime code changed.

The next review found stale removal-status summaries in README, CONTEXT and
the architecture guide. Current summaries now consistently state source removal;
ADR-0054 distinguishes its original maintenance decision from the follow-up,
and the interaction disposition points at this change's removal ownership.

Follow-up checks cover all non-archived Markdown links targeting files removed
by this diff. The research source link now pins its original Git revision;
the testing banner and research acceptance text use current terminal hosts.
The standalone guide links its completed migration's archive instead of a
nonexistent active change. No permanent documentation scanner was added.
