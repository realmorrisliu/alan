# Baseline verification

## Local documentation checks

- `openspec validate --all --strict`: 85 passed, 0 failed (75 specs, 10 changes),
  using installed OpenSpec 1.13.1. CI uses its repository-pinned version.
- `scripts/check-openspec-current-surfaces.sh`: passed.
- `git diff --check`: passed.
- Exact requirement-block comparison: all five baseline capability deltas match canonical.
- Byte comparison against origin/main: all four cancelled tasks.md files are
  unchanged; cancellation notes also record their SHA-256 values.
- Diff scope: Markdown and OpenSpec YAML only. No Rust/Swift source, build,
  dependency, executable script, installed state or release configuration change.
- The three strict delta errors were scenario-name mismatches: case change in
  the memory scenario and renamed execution scenarios. Stable names are restored;
  intended proposed Store/evaluator semantics remain in the parked drafts.

## Not claimed

CI compatibility check: OpenSpec 1.4.1 validates the first physical requirement
line for SHALL/MUST, unlike the installed 1.13.1 paragraph parser. The two
adapter requirements and their deltas put SHALL on that first line without
changing semantics. Both versions are used for the follow-up strict validation.

No Jev API test, terminal/Herdr end-to-end qualification, desktop removal, account
migration or sandbox modification. The old standalone inline-TUI guard remains
known stale (Ink false positive and old entry symbol); it is not repaired by
this documentation-only baseline. Runtime/source-removal planning owns that
guard's eventual replacement alongside the terminal entry contract.

## Delivery evidence

Codex review at 8e97351 found a valid P2: two non-desktop-specific contracts
still assigned native credential and mount adapter ownership to the retired
App. Both contracts and matching deltas now name the durable platform Host
adapter boundary, scope the retained App implementation as maintenance-only,
and gate removal on verified preservation of still-needed safety behavior.
This is a contract correction, not a claim that adapter extraction has shipped.
The shared root cause was conflating implementation carrier with durable owner.

The second review identified stale concrete-client naming in the App integration
contract. Its boundary remains valid, but new consumers must not inherit a
retired-client prerequisite. Both shared-authority and file-boundary clauses
now name generic Alan OS clients, scope the old macOS scenario to legacy
maintenance, and keep Herdr itself outside the aP-client obligation. No new
integration layer or weakening of direct file boundaries was introduced.

Local `just quality` passed before this docs-only follow-up; current-head
validation is repeated after it. The PR records the unrelated Cargo Audit
failure on main's unchanged rustls 0.23.36 dependency (RUSTSEC-2026-0285).

Full local quality and required current-head CI results are recorded in the PR.
PR #921 merged on 2026-09-19 at `3fce4450ad0b1d0412baeec095224d48bc7937d8`.
The final reviewed head was `a3a3db6c921c25a8cdbe6bf7582582ad9860b2b3`:
both review threads were resolved, the last Codex review reported no findings,
and all four required checks passed. Cargo Audit remained a separate failure.

Post-merge, the main tree exactly matched that reviewed head. All five delta
capabilities already matched canonical, so archival performs no additional sync.
Local main was fast-forwarded to the merge commit. The old #890 branch had an
identical tree to its merged squash commit, which is an ancestor of main; both
that branch and the #921 local branch were removed. Neither remote branch
remained. No other branches were deleted.

There is only the primary worktree `/Users/morris/Developer/alan`; it was retained.
The archive and next-planning index are prepared separately on
`codex/record-next-planning-tasks`, created from the updated main. That follow-up
PR's own merge and branch cleanup are not claimed complete here.
