## ADDED Requirements

### Requirement: Red-line detection is token-aware, never substring

Deterministic red-line and review gates SHALL classify shell commands by
whitespace/quote-normalized tokens (and command basenames), not by raw substring
matching. Equivalent shell forms — flag reordering and bundling, surrounding
quotes, alternate whitespace, leading-`+` refspecs, and path-qualified executable
heads — SHALL be classified identically to their canonical form. Substring
`match_command` rules in the policy engine MAY remain as a baseline but SHALL NOT
be the sole guard for any red line.

#### Scenario: Force-push in any token form routes to a human
- **WHEN** a builtin bash command is a `git push` with a force flag in any form (`--force`, `-f`, `--force-with-lease=…`, a leading-`+` refspec, quoted, or with `git -C …`)
- **THEN** it is routed always-human and the reviewer never decides it

#### Scenario: Irreversible git reset in any token form is reviewed
- **WHEN** a builtin bash command is a `git reset --hard` in any ordering or quoting (`git -C repo reset --hard`, `git reset HEAD --hard`, `git reset '--hard'`)
- **THEN** it is escalated for review instead of auto-approved

#### Scenario: Recursive delete in any flag form is reviewed
- **WHEN** a builtin bash command is a recursive `rm` in any flag ordering or bundling (`rm -rf`, `rm -fr`, `rm -R -f`, `rm --recursive`, `/bin/rm -fr`)
- **THEN** it is escalated for review instead of auto-approved

#### Scenario: Destructive find actions are reviewed
- **WHEN** a builtin bash command runs `find` with a destructive action (`-delete`, `-exec`/`-execdir`, `-ok`/`-okdir`)
- **THEN** it is escalated for review instead of auto-approved as a bare in-workspace write

#### Scenario: Catastrophic root delete is denied outright
- **WHEN** a builtin bash command is a recursive `rm` whose target is a filesystem or home root (`rm -rf /`, `rm -fr /`, `rm -rf /*`, `rm -rf ~`, `rm -rf $HOME`)
- **THEN** it is denied outright and never reaches the reviewer or the human

#### Scenario: Privilege escalation in any token form routes to a human
- **WHEN** a builtin bash command invokes a privilege escalator (`sudo`, `doas`, `pkexec`, `su`) in any whitespace form or via an absolute path (`sudo\tls`, `/usr/bin/sudo …`)
- **THEN** it is routed always-human even if another rule already escalated it to the reviewer

#### Scenario: World-writable chmod routes to a human
- **WHEN** a builtin bash command runs `chmod` granting write to others/all in any numeric or symbolic form (`777`, `0777`, `-R 777`, `a+rwx`, `o+w`, `a=rw`)
- **THEN** it is routed always-human; owner-only grants (`u+w`), read-only modes (`644`/`755`), and revocations (`o-w`) are not flagged

#### Scenario: Path-qualified tools classify by basename
- **WHEN** the bash classifier sees a path-qualified executable head (`/usr/bin/curl`, `/bin/rm`)
- **THEN** it classifies by the command basename so capability (and the resulting sandbox network intent) matches the bare-head form

### Requirement: The reviewer is not a security boundary

The autonomous reviewer SHALL decide only escalations the active sandbox can
contain. When the sandbox cannot confine an effect, the operation SHALL be routed
to a human (or denied) rather than left for the reviewer, and the reviewer SHALL
NOT be able to convert a sandbox-uncontainable operation into an executed one.

#### Scenario: Escalated bash goes to a human when not fully confined
- **WHEN** a builtin bash command that needs judgment (escalated — destructive like `rm -rf build`, irreversible like `git reset --hard`, or opaque/unknown like `cargo test`/`pytest`/`python script.py`) runs under a backend that does not FULLY confine bash (network unconfined, or protected-subpath writes not kernel-denied — Landlock or the path-guard fallback)
- **THEN** it is routed always-human, because the reviewer is not a security boundary for code the sandbox cannot fully contain; only Seatbelt (confines network + protected writes) keeps such commands reviewer-eligible

#### Scenario: Recognized benign bash still auto-runs
- **WHEN** a builtin bash command is auto-approved (a recognized read/write like `touch`, `echo`, `ls` whose path operands the parser confined to non-protected workspace paths)
- **THEN** it still runs without prompting even when the backend is not fully confining, because the path guard already contains it — the human gate applies only to escalated commands

#### Scenario: Reviewer timeout falls back to a human
- **WHEN** the reviewer provider stalls beyond the configured request timeout
- **THEN** the outcome is `Unavailable` and the operation falls back to a human, never auto-allowed

### Requirement: OS-sandbox confinement is independent of command syntax

The syntactic shape parser SHALL be dropped only for a backend that
kernel-enforces protected-subpath writes (Seatbelt); such a backend enforces
deterministic filesystem confinement at the kernel rather than via the
workspace-path-guard command parser. A backend that confines the workspace but
cannot carve out protected subpaths (Landlock) SHALL keep the full shape parser
so opaque writers cannot hide a protected write the kernel will not deny.
Protected-subpath writes SHALL remain blocked on every backend.

#### Scenario: Wrappers run under a protected-write-enforcing sandbox
- **WHEN** a backend that kernel-denies protected-subpath writes (Seatbelt) is active and a command uses a shell wrapper or interpreter (`bash -lc …`, `python -c …`)
- **THEN** it is not rejected by the syntactic preflight or execution-path shape parser; the kernel sandbox confines it (protected subpaths included)

#### Scenario: Opaque writers stay rejected under Landlock
- **WHEN** Landlock is active (it cannot carve a protected subdir out of the writable workspace) and a command is an opaque writer the path check cannot inspect (`python -c 'open(".git/config","w")…'`, `python scripts/setup.py`)
- **THEN** it is rejected by the shape parser, the same posture as the path-guard fallback, because the kernel cannot deny the protected write

#### Scenario: Direct/nested protected-subpath tampering is blocked
- **WHEN** a command writes to a protected subpath (`.git`, `.alan`, `.agents`) via an explicit path operand, directly or hidden inside a shell-wrapper inline script (`bash -lc 'echo x > .git/config'`)
- **THEN** the write is blocked by the path-guard parser, which checks direct operands and recurses into shell-wrapper inline scripts. The protected subpaths are NOT kernel-denied — denying `.git` would break git itself, which must write `.git` — so program-internal writes by purpose-built tools (git porcelain to `.git`, the agent to `.alan/memory`) are allowed

#### Scenario: Out-of-workspace reads stay contained under an OS sandbox
- **WHEN** an auto-approved read-classified bash command references a path outside the workspace (`cat ~/.ssh/id_rsa`, `cat /etc/passwd`), under any backend including a wrapper form
- **THEN** it is rejected by the path-guard parser's containment check — the OS sandbox confines writes and network but permits reads, so dropping the shape parser must NOT drop path containment; secrets cannot be read into tool output without approval

#### Scenario: Carve-outs are preserved under recursion
- **WHEN** an OS-sandboxed command writes to an agent-writable carve-out within a protected root (`.alan/memory`), directly or inside a wrapper
- **THEN** the write is allowed, because the recursive protected check honors the same carve-outs as the direct path check

#### Scenario: Approved network intent is preserved
- **WHEN** a command classified as a network capability is approved and executed
- **THEN** it runs with the sandbox network restriction lifted (still filesystem-confined) so the approved network call is not futile

### Requirement: The client never silently drops events across a reconnect

The TUI's live event stream SHALL preserve a replay cursor so that no event is
lost between hydration and the first subscribe, or across a disconnect and
resubscribe. The future-only `/events` stream SHALL be backstopped by draining the
buffered `/events/read` replay API from the last seen event id before each
(re)subscribe, with sequence-based dedup against any overlap.

#### Scenario: Events emitted during a reconnect gap are replayed
- **WHEN** the live `/events` stream errors or ends and the client waits and resubscribes, and one or more events (e.g. a `Yield`) were emitted during the gap
- **THEN** the client drains `/events/read` after the last seen event id and delivers the missed events before resuming the live stream, so a pending approval/form still appears

#### Scenario: Overlap between replay and live stream is deduped
- **WHEN** a drained buffered event and a live-stream event refer to the same sequence
- **THEN** it is delivered once, deduped by sequence

#### Scenario: A replay gap is surfaced, not silently dropped
- **WHEN** `/events/read` reports `gap: true` (the replay cursor had fallen out of the daemon buffer, so only a truncated tail is returned)
- **THEN** the client surfaces a recoverable error to the user instead of continuing as if replay were complete, so missed tool/approval state is not hidden
