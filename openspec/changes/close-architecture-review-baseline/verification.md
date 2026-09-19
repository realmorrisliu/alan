# Baseline verification

## Local documentation checks

- `openspec validate --all --strict`: 85 passed, 0 failed (75 specs, 10 changes),
  using installed OpenSpec 1.13.1. CI uses its repository-pinned version.
- `scripts/check-openspec-current-surfaces.sh`: passed.
- `git diff --check`: passed.
- Exact requirement-block comparison: both baseline deltas match canonical.
- Byte comparison against origin/main: all four cancelled tasks.md files are
  unchanged; cancellation notes also record their SHA-256 values.
- Diff scope: Markdown and OpenSpec YAML only. No Rust/Swift source, build,
  dependency, executable script, installed state or release configuration change.
- The three strict delta errors were scenario-name mismatches: case change in
  the memory scenario and renamed execution scenarios. Stable names are restored;
  intended proposed Store/evaluator semantics remain in the parked drafts.

## Not claimed

No Jev API test, terminal/Herdr end-to-end qualification, desktop removal, account
migration or sandbox modification. The old standalone inline-TUI guard remains
known stale (Ink false positive and old entry symbol); it is not repaired by
this documentation-only baseline. Runtime/source-removal planning owns that
guard's eventual replacement alongside the terminal entry contract.

## Delivery evidence

Full local quality and required current-head CI results are recorded in the PR.
Merge and archive readiness must be established before tasks 2.2/2.3 are checked.
