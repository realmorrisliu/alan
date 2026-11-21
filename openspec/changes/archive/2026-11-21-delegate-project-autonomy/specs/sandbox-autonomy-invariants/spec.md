# Spec Delta

## MODIFIED Requirements

### Requirement: Red-line detection is token-aware, never substring

Deterministic red-line and review gates SHALL classify shell commands by
whitespace/quote-normalized tokens (and command basenames), not by raw substring
matching. Equivalent shell forms — flag reordering and bundling, surrounding
quotes, alternate whitespace, leading-`+` refspecs, and path-qualified executable
heads — SHALL be classified identically to their canonical form. Substring
`match_command` rules in the policy engine MAY remain as a baseline but SHALL NOT
be the sole guard for any red line.

#### Scenario: Undelegated force-push in any token form routes to a human
- **WHEN** a builtin bash command is a `git push` with a force flag in any form (`--force`, `-f`, `--force-with-lease=…`, a leading-`+` refspec, quoted, or with `git -C …`)
- **THEN** absent an exact current user-origin force-push delegation, it is routed to a trusted-user decision and the reviewer never grants that authority
- **AND** ordinary push delegation does not satisfy force-push or remote deletion

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

#### Scenario: Reviewer aggregate fallback budget exhausts
- **WHEN** the reviewer and permitted main-model fallback cannot decide within the aggregate deadline and call budget
- **THEN** the outcome remains unavailable or uncertain and one correlated human request remains, never auto-allowed
