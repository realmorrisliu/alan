# Final adoption and archive receipt — 2026-10-14

PR #1059 merged as `f2a98f4b` and is an ancestor of current main `ea929481`.
Reviewed head `1c4a7aa3` passed all sixteen checks, completed automatic review
without new findings, and resolved all six review threads. Its final TUI suite
passed 407 tests (395 unit and 12 integration); original native evidence and
its limitations remain recorded below.

This documentation worktree synchronizes the merged contextual-prompt delta to
`rust-inline-tui`, preserving subsequent canonical requirements, and prepares
this change for archive. The implementation is merged; this new canonical-sync
and archive patch still requires its own review and merge. Further observed UI
work belongs to `qualify-personal-daily-development`, without reopening delivered
work or claiming additional native acceptance.

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

## Review follow-up

PR #1059 review exposed two compact-layout regressions: overlapping model control hid required-response cues, and queue metadata evicted execution actions. Both new regression tests failed on the reviewed implementation before the fix. Compact attention now prioritizes approval/form responses and binds running/paused labels to Esc stop or /continue, using at most two physical rows before dropping secondary metadata.

- Updated terminal UI suite: 403 passing tests (391 unit, 12 integration). Production TestBackend coverage includes 16/24/40 columns with uncertain, paused, deferred, pending, active and unknown queue observations plus pending/uncertain model selection; the 16-by-6 unknown-queue case retains model, action and warning across wrapping.
- Strict change validation and independent Spec/Standards review pass for the follow-up.
- Fresh debug build in the same isolated Herdr fixture completed a real read-only hello.txt task. Running exposed Esc interrupt and completion returned to quiet context; rare overlapping states are covered by the production buffer tests, not claimed as live-model observations.
- Follow-up commit/check/review state remains authoritative on the PR.

A subsequent review found unsupported /queue hints in both wide and compact attention. All four presentation hints now use the existing /status command; the proposal was corrected to name that actual surface. A regression invokes the command extracted from the rendered hint at narrow/wide widths for uncertain, deferred and unavailable observations, verifies the matching queue details and verifies no work action is emitted. It reproduced unknown-command failure before the fix. The complete TUI suite now passes 404 tests (392 unit, 12 integration).

Response/queue distinction follow-up: pending approval/form state or waiting submission IDs suppress the queue-only /continue hint at every width. One shared presentation predicate also keeps activity text and repaint timing static if the activity observation arrives before the request controls. Expanded existing response regressions reproduced both the narrow waiting-ID case and the wide approval-plus-paused-queue case before the fix. Ordinary paused queues retain /continue.

Short-form and asynchronous model follow-up: the production viewport now follows the focused form field even without a composer cursor, scrolling earlier rows out when needed. Actual-buffer tests exercise typing, focus cycling and 1/2/4/8-row panes at 24/40 columns with a wrapped preceding notice. A known active queue now selects the active model even before the activity event; missing active stays unknown and an observed settled queue restores selected-next. Both regressions failed before their fixes. The full TUI suite now passes 406 tests (394 unit, 12 integration).

Independent review also caught fields taller than the pane: the viewport now follows the editable value end while excluding trailing option help, with shared FormState formatting for measurement/rendering. The added real-buffer regression reproduced hidden edits before the fix and covers long labels, Chinese/emoji values and long option lists. Final TUI suite: 407 passing tests (395 unit, 12 integration); both review axes pass after re-review.
