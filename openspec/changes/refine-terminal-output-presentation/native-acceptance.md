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
