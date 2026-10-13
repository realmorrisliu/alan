# Delivery evidence

Base: `efba3346` (merged installation fix #1058). Scope: terminal presentation only.

- Terminal UI suite: 401 tests pass (389 unit, 12 integration). Includes cursor/Unicode/scrollback, completion anchoring, actual 16-column short-buffer paused/uncertain and running/unknown-queue matrices.
- Strict OpenSpec change validation passes.
- Independent Standards review: PASS. Spec review found narrow-state clipping and missing-active-model fallback ambiguity; both fixed, including a subsequent unknown-queue compact-label case, and re-review PASS.
- Native Herdr pane `w58:p1C`, isolated fresh-onboarding HOME and XDG_DATA_HOME. The local debug binary was freshly rebuilt; no personal installation or credentials changed.
- Real read-only project task read `hello.txt` and returned `Alan onboarding OK`. Running showed model/context plus `Working · Ns · Esc interrupt`; completion removed ready/zero queue/running status. Stable idle reads `project · read-only · gpt-6.1-sol / low` above `: `, separated from history by a blank row and rule.
- `/status` exposes namespace cwd, retained project/access/mount, selected-next/active/catalog/admitted observations and full queue state. It does not manufacture a Host path from a namespace cwd.
- Detailed local evidence is under the isolated onboarding fixture's `evidence/composer-status/`. Native observations use Herdr's terminal buffer; screenshot access to Ghostty is unavailable. Smallest-pane state combinations are verified with the real production renderer's TestBackend, not a claim of physical-terminal coverage for every state.

Native resize to 42 columns and per-character `/project` input passed with a stable two-row rule-to-input distance; original pane width restored. The final source suite has 401 passing tests. Complete quality and standalone distribution gates passed after the reviewed fixes, including the normal pre-commit hook. Published [PR #1059](https://github.com/realmorrisliu/alan/pull/1059); initial implementation head `22242d9c` has CI queued/running (not a pass claim). Current-head CI and review evidence is authoritative on the PR, allowing this checked-in delivery snapshot to stay stable. No merge or canonical adoption is claimed.
