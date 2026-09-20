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

Review, CI, merge, canonical sync and archive remain unchecked in tasks.md.

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
