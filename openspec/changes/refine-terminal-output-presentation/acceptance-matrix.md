# Frozen native UI workflow matrix

Source: `a76aa15dc15eefcdde1973bf5f59559418f5beb0`; fresh macOS aarch64 debug
binary SHA-256 `e2cc928b8306434e42dae50dbb6091d6223f4bfccb4f237f9a770408b3f987bb`.
Profile: `chatgpt-main`, `gpt-6.1-sol`, medium. Each slot uses an independent dev
invocation, explicit project approval and an owned disposable fixture. Broader
Linux/toolchain and repeated development qualification are separate deliveries.

## Fixtures and effect assertions

Fixture root: `~/Library/Caches/Alan/ui-native-matrix-20261009/`.
Six independent directories are named by host/width below, each initialized at
Git baseline `4ffdf7aeac6f4b2abe452865e393e57a16bb7945` with README.md (3 lines),
sample.rs (2 lines) and pager.txt (180 lines). Initial hashes match the fixture
in `native-acceptance.md`. Expected changes are only the explicit edit and effect
ledger below; verify actual bytes and Git diff before counting a slot as passed.

| Workflow | Frozen task and interaction | Required observations and effects |
| --- | --- | --- |
| Read/search burst | Read all three files once and search literal `server>`; no writes or shell. Inspect distinct members with Ctrl+O/Left/Right, Space/b and Esc around a Chinese/emoji draft. | Successful positively eligible neighboring reads may group; exact selected originals; no file changes; same draft/cursor. |
| Commands | Explicit success prints Chinese/emoji/literal `server> ready` and appends one `command-once` line to effects.log. Separate explicit failure prints distinguishable stdout/stderr and exits 7. | Standalone compact status, actual exit 0/7 and separate streams in detail; exactly one ledger line. |
| Edit/diff | Use edit_file once to change sample.rs `a + b` to `a - b`, preserving its comment and other files. Request a rust code block and a quoted literal in the answer. | Standalone edit status and inspectable +/- diff; exact one-line source diff; readable code boundaries/answer spacing; literal comment preserved. |
| Approval/cancel/resume | Cancel the project selector before changing authority; run a separate bounded waiting command with only a delayed effect and queue a second command appending `resumed-once`. Interrupt before the delayed effect, revoke project access, explicitly reapprove and only then /continue. Separately attempt /continue without authority as a negative control. | Request choice and cancelled/paused/blocked states stay distinct; delayed cancelled effect absent, earlier `command-once` stays exactly once; queued `resumed-once` executes exactly once only after explicit reapproval plus continue. For an unrecovered Root, an unauthorized continue may consume the queued task and fail at Tool authorization; durable recovery's preflight/retention is a separate contract. Record actual behavior, not inferred success. |
| Long output/follow-up | Read pager.txt, inspect/paginate through line 180 and original bytes, then ask for its final line and current sample.rs behavior. | Correct correlated retained content, usable scrollback, distinct old/current details, no additional edits or duplicated ledger effects. |

## Required slots

All five workflows run in each of six host/width slots (30 workflow slots).
Earlier 73-column Herdr and 48-column targeted PTY captures are valuable smoke
evidence but do not fill this fresh candidate's full matrix. A timeout, refusal,
assisted correction or unsupported operation is recorded as such. First attempts
and corrected reruns remain separate; raw ANSI/Host captures accompany results.

| Host / columns | Fixture | Reads | Commands | Edit/diff | Authority lifecycle | Long follow-up |
| --- | --- | --- | --- | --- | --- | --- |
| Ordinary PTY / 48 | pty48 | passed with corrected supported task | passed | passed after repair | positive flow and negative control passed | passed |
| Ordinary PTY / 80 | pty80 | corrected: search Tool unavailable | passed | failed: duplicate path | negative control plus corrected positive flow | passed |
| Ordinary PTY / 120 | pty120 | pending | pending | pending | pending | pending |
| Herdr / 48 | herdr48 | pending | pending | pending | pending | pending |
| Herdr / 80 | herdr80 | pending | pending | pending | pending | pending |
| Herdr / 120 | herdr120 | pending | pending | pending | pending | pending |

Terminal height, runtime handle, original/corrected task text, duration, response,
artifact checks and no-repeat assertions are recorded per actual slot. Existing
deterministic fixtures cover remaining inventory states, including unknown Tool
fallback, unavailable evidence and superseded attachment replies. Those tests do
not become native mixed-Machine or autonomous-development qualification.

## Ordinary PTY / 80: first candidate

Terminal: 80 columns × 22 rows; concrete Root `/agent/8`, runtime
`~/Library/Caches/Alan/o9m80`. Source/binary/profile are the frozen identities above.
The owned invocation exited through `/quit` with status 0. Ignored raw ANSI
chunks `target/m80-01-project.json` through `m80-46-continue-authorized.json`
were replayed through the existing `vt100` dependency. Observation windows
include capture delay and are not model-runtime latency measurements.

- Read task asked for three single reads and an independent grep without shell.
  The default Root's existing core Tools omit grep; the model truthfully reported
  that limitation and answered 3/2/180 lines from the three reads. Reads grouped
  and Ctrl+O/Left selected distinct originals. Esc restored `矩阵草稿 中文😀`
  with cursor row 20, column 17 (zero-based). A separate explicit
  `! grep -n 'server>' sample.rs` returned `2:// server> ready` with exit 0 and
  remained standalone. This assisted correction is not a first-attempt pass.
- Explicit success printed Chinese/emoji and `server> ready`, then appended one
  `command-once` line. Separate failure printed distinguishable stdout/stderr
  and exited 7. Retained detail preserved both streams, exit code and concrete
  source; Esc restored `命令草稿 中文😀`.
- One relative-path edit changed only `a + b` to `a - b`; retained +/- detail
  and rust code boundaries were usable. The summary repeated the same file as
  relative title and absolute result path. This is a failed display acceptance,
  despite correct artifacts. Runtime metadata now reuses the resolved result
  path for known file Tools; RED/GREEN and fresh native reacceptance are separate.
- Initial active-grant `/project` refusal was not selector cancellation. After
  explicit revoke, entering the actual selector and Esc did cancel it. A waiting
  command was interrupted with a queued successor; no delayed tail was written.
  `/continue` without authorization consumed the successor and produced a Tool
  authorization failure, with no effect. This matches existing unrecovered-Root
  dispatch behavior; it does not qualify durable recovery's paused retention.
  In a fresh submitted pair, interrupt → revoke → reapprove kept the successor
  paused until explicit `/continue`, which appended `resumed-once` exactly once.
  Reapproval alone and cancellation produced no ledger effects. The revocation
  notice is being clarified to explain this order without changing queue policy.
- A follow-up read returned exact pager line 180 and the subtraction function.
  Ctrl+O/Space reached lines 173–180 and original result bytes. Plan detail `p`
  showed both exact historical snapshots (in-progress and completed), preserving
  each explanation and status, rather than substituting the newest plan.

After exit and beyond the cancelled commands' delay, assert-based artifact
checks passed: README.md and pager.txt byte-identical to baseline; sample.rs
exactly one replacement; effects.log exactly `command-once\nresumed-once\n`;
tracked diff only sample.rs and untracked files only effects.log. Neither
cancelled tail nor a repeated earlier command appeared.

## Ordinary PTY / 48: repaired candidate

Source: `a76aa15d` plus the resolved-result title and revocation-notice repair
recorded in `implementation-notes.md`; debug binary SHA-256
`3e9452d24ccabb1612bd6ab90b85eba71f084f610fa78b83c1991c91a3641834`.
Fresh untouched fixture `pty48`, 48 columns × 22 rows, runtime
`~/Library/Caches/Alan/o9f48`, concrete Root `/agent/8`, same model/profile/effort.
The owned invocation exited through `/quit` with status 0. Receipts are ignored
raw ANSI chunks `target/f48-01-project.json` through `f48-43-exit.json`.
This full slot belongs to the repaired candidate, not the original 80-column
candidate; the other five host/width slots still require this candidate's runs.

- Actual project selector Esc cancelled without approval; explicit read-only
  selection then allowed three single reads. The corrected task asked to find
  `server>` in the read content, without the unavailable standalone grep Tool.
  Correct line counts 3/2/180, a three-member successful read group, distinct
  pager/sample originals, Space paging and exact literal text were observed.
  Esc restored `窄终端草稿 中文😀`, cursor row 19, column 19. After explicit
  read-write selection, separate `! grep -n 'server>' sample.rs` returned the
  exact match and stayed standalone. This is a corrected supported task, not
  a first-attempt pass of the original unsupported task.
- Explicit success and exit-7 failure retained actual status, literal Chinese/
  emoji and distinguishable stdout/stderr in detail. The success wrote one
  `command-once` line. One model edit changed exactly the requested operation;
  the default summary showed the resolved path once and only `+1 -1` underneath.
  Complete +/- detail and rust/text code boundaries remained available.
- Waiting work plus one queued command was interrupted; the queue paused.
  Revoke showed `use /project before /continue for project work` at narrow width.
  Explicit reapproval left the queue paused and effects unchanged. Only
  `/continue` appended one `resumed-once` line. A separate interrupted pair
  followed by revoke and unauthorized continue failed before the queued effect;
  it did not reuse revoked authority or replay earlier work. Cancelled effects
  and the unauthorized marker were absent.
- The long-output follow-up read each file once, returned exact line 180 and
  subtraction behavior, and produced three distinct compact plan records.
  Space reached lines 172–180 and original raw bytes; plan navigation showed
  the completed snapshot and its distinct earlier in-progress explanation.
  Esc restored `后续草稿 中文😀`, cursor row 19, column 17.

Assert-based artifact checks after exit and beyond the cancelled delay passed
the same byte/diff/ledger conditions as the 80-column run, including absence of
the negative-control effects. This run is native UI
acceptance with existing core Tools, not repeated real-development qualification.
