# Partial native acceptance — 2026-10-09

This is partial acceptance, not the five-workflow matrix or release qualification.

## Candidate and environment

- Source: `6d89fda9`, including the historical-plan slice; macOS aarch64 debug CLI.
- Binary SHA-256: `5eb2a2a7723011a112f31381807cb19e2d0487e614363088239dc913ec971346`.
- Herdr pane created for this test: `w58:p0`, 73 columns, 21 rows. Focus remained
  with the user. The owned Alan process exited through `/quit`; the test pane
  was subsequently closed. Other panes were untouched.
- Runtime: `/Users/morris/Library/Caches/Alan/o9h`; the initially longer test
  runtime path was rejected by the macOS socket-length guard before startup.
- Profile: `chatgpt-main`; the actual status displayed `gpt-6.1-sol`, `medium`.
- Fixture: `~/Library/Caches/Alan/output-presentation-20261009/project`.
  `sample.rs` SHA-256 `0cf84c77a039ee5448cbb0462937154e2c03d409a726534a1a9f4a9c911d9d79`;
  `README.md` SHA-256 `ebe26ef63d2c015dd7d0b253fb7579dff20e3fecbd3889088ad527e861116ea1`.
  Both hashes were unchanged after the real-model tasks.

## Observed behavior

- Explicit command `printf "server> ready\n"; printf "stderr marker\n" >&2;
  exit 7` under the disposable read-write project produced a compact failed
  Action with `exit 7 · stderr marker`. Individual retained detail exposed
  distinct stdout/stderr and the exact literal `server> ready`. Earlier attempts
  under the read-only grant were refused before execution; they are separate
  failed attempts, not successful command qualification.
- The model produced four compact plan revisions across two read-only tasks.
  Ctrl+O, then `p`, displayed exact retained snapshots for `/agent/8`; navigating
  backward showed revision 1's original explanation and incomplete steps, while
  revision 4 showed completed steps. Returning kept the draft `草稿😀保持`.
  Exact cursor preservation remains backed by deterministic tests, not a native
  cursor-coordinate claim.
- The second task read both fixture files successfully and returned a `rust`
  code block plus an exact literal quote. No file mutation or shell execution
  was requested from the model.

## Findings requiring follow-up

1. After revoking and replacing a project grant, the first model task used the
   old `/mnt/project-request-1/` path. Both reads were correctly rejected; the
   task did not complete. Giving the current `/mnt/project-request-2/` path in
   the second user message allowed successful reads. Do not count that assisted
   success as first-attempt success or claim directory-context propagation is
   qualified. Trace the model-visible current Process directory before repeats.
2. Committing Chinese/emoji rows to scrollback introduced spaces between wide
   characters and lost row tails. The installed Ratatui `insert_before` full-cell
   path emits covered cells, unlike ordinary frame diffs. A new backend/VT parser
   regression reproduced the corruption. The subsequent adapter repair emits
   no bytes for covered cells; fresh native reacceptance is recorded below.

Local ignored receipts: `target/native-real-task.txt`,
`target/native-plan-old.txt`, `target/native-plan-latest.txt`,
`target/native-draft-return.txt`, and `target/wide-regression-before.log`.
Native screenshot access to the terminal application was unavailable; these
observations use Herdr's terminal surfaces and deterministic backend evidence.

## Wide-character repair: fresh native reacceptance

The repaired debug binary SHA-256 is
`a6e4a3ce05a447d1534eb4255f4e4672168f0f159a95ac7be33317d27b2a230a`.
An owned fresh Herdr pane `w58:p11` ran six distinct explicit command submissions
containing Chinese, emoji and literal `server>ready`, enough rows to enter Host
scrollback. With no selected project all six were correctly refused before
execution; this qualifies rendering of those refusals, not shell execution.
Herdr's recent-unwrapped capture preserved `宽字符😀编号1` through revision 6 and
`中文计划读取文件检查结果` without inserted spaces or lost row tails. Receipt:
`target/native-wide-after.txt`. The owned invocation exited through `/quit`.

The adapter suite passed 349 library and 12 integration tests; the focused native
backend test covers 48/80/120 columns. The fresh full `just quality` gate and
strict OpenSpec validation (66/66) passed. Ordinary PTY and the complete native
workflow/width matrix remain open. Model-visible directory propagation was still
unqualified on this candidate.

The subsequently discovered model-directory gap has a separate repair and a
successful targeted native rerun recorded in
`../refresh-model-process-directory/acceptance.md`. The original failed task
above remains part of the baseline; it is not reclassified as a pass.

## Notice and detail targeted native acceptance

Fresh binary SHA-256 `132f8f2196b4917e2b7e2638053a5ad07f4a9e71437d0e9a7c05747493ea9c10`:
source `240496a2` plus the core notice slice, before the final local model/project
severity refinements. Those final branch refinements have deterministic tests;
this receipt does not claim a fresh native run of them or the PR review correction.
Owned pane `w58:p13`, 73×21, runtime `~/Library/Caches/Alan/o9n`, unchanged user
focus `w58:pZ`; `chatgpt-main`, effective `gpt-6.1-sol`/`medium`.

Selected the same disposable project read-only and asked to read both fixture
files without supplying namespace paths. Two reads and the answer succeeded;
`queued 0` appeared once, in the context header, with no duplicate notice row.
Ctrl+O opened the README result; Left selected the distinct `sample.rs` result.
Both retained literal `server> ready` and `a > b`. Esc restored the unsubmitted
`保留草稿 中文😀`. This verifies selected member content and draft text; cursor
identity is covered by deterministic tests. Space/b were sent, but immediate
captures did not establish a changed viewport, so native page aliases remain
unqualified. Receipts: `target/native-notice-success.txt`,
`target/native-notice-detail-readme.txt`, `target/native-notice-draft.txt`.

Executed the harmless explicit commands `printf ui-notice-failure` and `false`:
respectively completed/exit 0 and failed/exit 1 remained distinct. The marker name
is literal command output; the first command was successful. The failed Tool
remained in permanent history after settlement. Receipt:
`target/native-notice-failure.txt`; this does not exercise an Engine-level failure
or unknown outcome. Fixture hashes matched the prior baseline. `/quit` ended
Alan; the foreground returned to the owned fish PID 96637 before closing the pane.
No self-development instance was left alive. The complete five-workflow native
matrix, ordinary PTY, grouping and broader qualification remain open.

## Read-only groups: fresh targeted acceptance

The first group candidate, binary SHA-256
`792e872c1caf64325cca60ab2acaa30b1b4fe2a4e50fcdd9f52516a73f45daaf`,
completed three reads but did not group them. Retained result metadata did contain
concrete `/agent/8` correlation; comparison against the renderer's moving alias
was the cause. This attempt remains an unqualified group run, not a pass.

After the Root-owner repair, fresh CLI SHA-256
`8aecb43544556ccffaf084113c2ef0ddfbfeb41ba5707eeca60abf9ae4bde49e`
(source `d7bf9b65` plus the renderer slice) ran in the same owned Herdr pane
`w58:p14`, 73×21, new foreground runtime `~/Library/Caches/Alan/o9g2`.
The user focus remained `w58:pZ`; model `gpt-6.1-sol`, effort `medium`.
A fresh read-only grant selected the disposable fixture. Without explicit
namespace paths in the prompt, the model read `sample.rs`, `README.md`, and all
180 lines of `pager.txt` and answered their line counts. The UI displayed
`3 read-only actions · completed` with three ordered concrete member rows.

Ctrl+O selected `pager.txt`; Space advanced from its header/line 001 to a view
containing line 020, and `b` returned to `Action a3`/line 001. Left twice selected
`sample.rs` (`Action a1`), with its own contents and result metadata. Esc restored
`分组草稿 中文😀`. Cursor identity remains deterministic-test evidence. This
qualifies same-Process member navigation and Host page aliases, not navigation
of older Process groups after replacement.

Explicit successful `printf` retained `exit 0 · stdout UI marker server> ready`
in its default summary. A compound stderr/exit command under the read-only grant
was refused before execution with `Working directory outside host_mount roots`;
it does not qualify native stderr/exit behavior under that grant. After explicit
revoke and read-write re-selection, the same requested command executed and
its standalone failed summary showed `exit 7 · stderr failure marker`.
The earlier refusal remains a separate failed attempt.

All fixture hashes remained unchanged. Existing README/sample hashes above
still apply; added owned pager fixture SHA-256 is
`4117a5063a1017c2d2a62e6b5a40e950b087959bf2315d2ed3184b97f77e84cf`.
The first invocation exited before rebuilding; the fresh invocation also exited
via `/quit`. Receipts are ignored local files:
`target/native-group-first-unqualified.txt`, `native-group-success.txt`,
`native-group-page-forward-fresh.txt`, `native-group-member-sample.txt`,
`native-group-draft.txt`, `native-group-stdout.txt`,
`native-group-command-failure-rw.txt`, and `native-group-final.txt`
(all under `target/`). The two 10-second group waits timed out before later
captures showed settled results; those waits are not successful lifecycle checks.
These are targeted Herdr observations, not the full PTY/Herdr width matrix or
instrumented once-only effect qualification.
