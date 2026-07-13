# sandbox-autonomy-invariants Specification

## Purpose
Defines security invariants shared by autonomous review and OS sandboxing,
including token-aware red lines, reviewer limitations, command-independent
confinement, and resumable renderer file streams.
## Requirements
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
- **THEN** it is escalated for review instead of auto-approved as a bare write
  within a writable Host Mount

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
- **WHEN** a builtin bash command is auto-approved (a recognized read/write like `touch`, `echo`, `ls` whose path operands the parser confined to non-protected Host Mount paths)
- **THEN** it still runs without prompting when its operands are confined to
  non-protected Host Mount paths, because the path guard already contains it —
  the human gate applies only to escalated commands

#### Scenario: Reviewer timeout falls back to a human
- **WHEN** the reviewer provider stalls beyond the configured request timeout
- **THEN** the outcome is `Unavailable` and the operation falls back to a human, never auto-allowed

### Requirement: OS-sandbox confinement is independent of command syntax

The syntactic shape parser SHALL be dropped only for a backend that
kernel-enforces protected-subpath writes (Seatbelt); such a backend enforces
deterministic filesystem confinement at the kernel rather than via the
Host Mount path guard. A backend that confines writable Host Mount roots but
cannot carve out protected subpaths (Landlock) SHALL keep the full shape parser
so opaque writers cannot hide a protected write the kernel will not deny.
Protected-subpath writes SHALL remain blocked on every backend.

#### Scenario: Wrappers run under a protected-write-enforcing sandbox
- **WHEN** a backend that kernel-denies protected-subpath writes (Seatbelt) is active and a command uses a shell wrapper or interpreter (`bash -lc …`, `python -c …`)
- **THEN** it is not rejected by the syntactic preflight or execution-path shape parser; the kernel sandbox confines it (protected subpaths included)

#### Scenario: Opaque writers stay rejected under Landlock
- **WHEN** Landlock is active (it cannot carve a protected subdir out of a writable Host Mount) and a command is an opaque writer the path check cannot inspect (`python -c 'open(".git/config","w")…'`, `python scripts/setup.py`)
- **THEN** it is rejected by the shape parser, the same posture as the path-guard fallback, because the kernel cannot deny the protected write

#### Scenario: Direct/nested protected-subpath tampering is blocked
- **WHEN** a command writes to a protected subpath (`.git`, `.alan`, `.agents`) via an explicit path operand, directly or hidden inside a shell-wrapper inline script (`bash -lc 'echo x > .git/config'`)
- **THEN** the write is blocked by the path-guard parser, which checks direct operands and recurses into shell-wrapper inline scripts
- **AND** purpose-built owners such as git porcelain may still write their own protected trees, while Agent memory writes use Memory Store files rather than raw Host backing paths

#### Scenario: Out-of-namespace Host reads stay contained under an OS sandbox
- **WHEN** an auto-approved read-classified bash command references a Host path outside every explicit mount (`cat ~/.ssh/id_rsa`, `cat /etc/passwd`), under any backend including a wrapper form
- **THEN** it is rejected by the path-guard parser's containment check — the OS sandbox confines writes and network but permits reads, so dropping the shape parser must NOT drop path containment; secrets cannot be read into tool output without approval

#### Scenario: Service-owned state is not a Tool carve-out
- **WHEN** an OS-sandboxed Tool command targets a raw System Store or Host Store backing path
- **THEN** the path guard rejects the write unless that exact path was separately authorized as a Host Mount
- **AND** ordinary Memory Store writes continue through mounted files and descriptors rather than a hidden sandbox carve-out

#### Scenario: Approved network intent is preserved
- **WHEN** a command classified as a network capability is approved and executed
- **THEN** it runs with the sandbox network restriction lifted (still filesystem-confined) so the approved network call is not futile

### Requirement: Renderer file streams preserve offsets across reattachment

A renderer reading an offset-addressable AgentFS stream SHALL retain its last delivered offset and SHALL NOT silently omit data when reopening the file or reattaching to the Agent Process. Overlap SHALL be deduplicated by stable file offset or record identity, and an unrecoverable retention gap SHALL be surfaced.

#### Scenario: Records written during reattachment are read

- **WHEN** a renderer's file watch ends and records are appended before it opens the stream again
- **THEN** the renderer resumes from its last delivered offset
- **AND** it delivers retained records in order before following new appends

#### Scenario: Snapshot and stream overlap is deduplicated

- **WHEN** hydrated snapshot state and an offset-readable stream contain the same durable record
- **THEN** the renderer presents the record once using its stable identity or offset

#### Scenario: Retention gap is surfaced

- **WHEN** the requested offset is older than retained stream data
- **THEN** the renderer reports a recoverable gap instead of pretending the stream is continuous
- **AND** recovery proceeds through current AgentFS snapshot and file semantics
