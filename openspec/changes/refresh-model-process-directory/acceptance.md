# Local qualification — 2026-10-09

## Reproduction and deterministic checks

The new assertion on actual requests after file-based directory selection failed
on the prior source: the replacement `/mnt/new` was absent from generation context.
Receipt: `target/directory-red.log`. After the repair, all 12 directory-selection
tests passed, including file and API controls and paused recovered work.

The initial isolated main-based branch's full Agent Execution Engine suite passed 1,403
tests with one existing ignored test, plus 20 dependency-boundary integration
tests. Both new directory-context tests passed. These capture actual requests across
changed bindings and between Tool iterations, verify JSON-escaped newline/quote/
emoji paths, omit missing/non-UTF-8 bindings, account for prompt overhead and
preserve historical Tape. The intra-turn test changes the binding in a fixture
Tool-completion callback; it proves request refresh, not live Host Mount mutation.
Receipt in the process-directory worktree: `target/engine-tests.log`. Earlier reproduction and
focused receipts remain in the output-presentation worktree's ignored `target/`.

## Native corrected rerun

- Candidate source: `8b7bbfe8` plus this repair; debug binary SHA-256
  `4143aea51615504611ea05fed3c1dd371848ee8e9dba4b51d5c6bb37a4b679c6`.
  This native candidate also contains the pending terminal-presentation changes;
  the four Runtime files at the initial isolated commit `d0e2d564` are identical to it.
- Owned Herdr pane `w58:p12`, 73 columns by 21 rows, no user focus change.
- Runtime: `~/Library/Caches/Alan/o9h`; profile `chatgpt-main`, status displayed
  `gpt-6.1-sol` with `medium` effort.
- Disposable project: `~/Library/Caches/Alan/output-presentation-20261009/project`.

Mounted the fixture read-only, asked to read `sample.rs` and `README.md` and explain
`add`, then revoked the grant and selected the fixture read-only again. Asked to
read the current project again without including either namespace path in the
user messages. Both tasks succeeded on their first attempt in this corrected run.
The first used `/mnt/project-request-1/`; the second used
`/mnt/project-request-2/`, with no obsolete-path refusal. Four reads completed;
no shell or file writes were requested. The earlier failure remains a failure
sample; this rerun does not erase it or qualify the wider repeated-task matrix.

Post-run SHA-256 values match the pre-existing fixture:
`sample.rs`: `0cf84c77a039ee5448cbb0462937154e2c03d409a726534a1a9f4a9c911d9d79`;
`README.md`: `ebe26ef63d2c015dd7d0b253fb7579dff20e3fecbd3889088ad527e861116ea1`.
Receipt: `target/native-directory-after.txt`. The owned invocation exited via
`/quit`; no self-development instance was left running.

## Delivery limits

The combined UI candidate's commit gate passed (OpenSpec 67/67). The isolated
main-based branch also passed full `just quality` and strict OpenSpec validation
(66/66), with receipts `target/quality.log` and `target/openspec.log`. No current-head
CI, merged delivery or general model reliability guarantee is claimed. One
selected-directory instruction provides context; live authority still decides
whether a Tool can access or execute anything.

## PR #1043 review correction

The review-correction receipts below belong to the isolated
`process-directory-20261009` worktree; native receipts belong to
`output-presentation-20261009`.

The review found that a missing PID binding could fall back to a global standalone
Tool binding. An actual-request regression with a default-only registry failed
before the correction (`target/default-binding-red.log`). The shared Process
lookup now returns only that PID's binding. All namespace Runtime context callers
use this lookup; standalone invocation's separate configured fallback remains
unchanged. Full Runtime tests passed 1,404 library tests, one existing ignored,
and 20 integration tests (`target/engine-review-tests.log`). Fresh full quality
and strict OpenSpec validation passed (`target/review-quality.log`,
`target/review-openspec.log`). The earlier native rerun exercised explicit PID
bindings; this additional missing-binding case is deterministic qualification,
not a new native run or current-head CI pass.
