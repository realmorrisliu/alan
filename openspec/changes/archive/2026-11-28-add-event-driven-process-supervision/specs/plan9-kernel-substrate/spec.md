# Spec Delta

## ADDED Requirements

### Requirement: Process exit observation uses a race-free /proc/<pid>/wait stream
`/proc/<pid>/wait` (and `/proc/self/wait`) SHALL be exposed as a read-only
`FileKind::Stream` backed by `alan_ap::Stream`. Reading blocks while the
process is alive and returns a single terminal JSON exit record on termination
or immediately from offset `0` if the process has already exited.

#### Scenario: Live wait unblocks immediately upon process exit
- **WHEN** a supervisor reads `/proc/<pid>/wait` at offset `0` while process `<pid>` is running and `<pid>` subsequently exits
- **THEN** the blocked read completes immediately with a single terminal JSON line containing `pid`, `status`, `exit_code`, and `reason`
- **AND** no periodic polling timer is required

#### Scenario: Attaching to wait after exit remains race-free
- **WHEN** process `<pid>` finishes before any consumer reads `/proc/<pid>/wait`
- **THEN** a subsequent read from offset `0` succeeds immediately with the retained terminal JSON record
- **AND** write-intent opens on `/proc/<pid>/wait` fail at dial time with `ErrorCode::NoAccess`

### Requirement: ExecSpec supports declarative link_parent cascade cancellation
`ExecSpec` SHALL include `link_parent: bool` (defaulting to `false`). When
`link_parent` is `true`, exiting the spawning parent process for any reason
(normal completion, failure, or cancellation) SHALL deterministically cancel
all linked child processes in `alan-kernel`.

#### Scenario: Linked child is cancelled when parent exits normally or abnormally
- **WHEN** a child process is spawned with `link_parent: true` and its parent exits
- **THEN** the kernel cancels the linked child process and publishes termination on `/proc/<child_pid>/wait`
- **AND** processes spawned with `link_parent: false` remain unaffected by parent exit
