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

#### Scenario: Bash without network confinement goes to a human
- **WHEN** a builtin bash command would otherwise be reviewer-eligible but the active backend does not confine network (no OS sandbox, or Landlock on a kernel without network rules)
- **THEN** the command is routed always-human, because bash can open sockets the sandbox would not contain

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

#### Scenario: Protected-subpath writes are blocked under an OS sandbox
- **WHEN** an OS-sandboxed command writes to a protected subpath (`.git`, `.alan`, `.agents`), directly or hidden inside a shell-wrapper inline script (`bash -lc 'echo x > .git/config'`)
- **THEN** the write is blocked: the Seatbelt profile kernel-denies protected-subpath writes, and the protected-only path check recurses into shell-wrapper inline scripts and re-applies the protected-subpath check

#### Scenario: Carve-outs are preserved under recursion
- **WHEN** an OS-sandboxed command writes to an agent-writable carve-out within a protected root (`.alan/memory`), directly or inside a wrapper
- **THEN** the write is allowed, because the recursive protected check honors the same carve-outs as the direct path check

#### Scenario: Approved network intent is preserved
- **WHEN** a command classified as a network capability is approved and executed
- **THEN** it runs with the sandbox network restriction lifted (still filesystem-confined) so the approved network call is not futile
