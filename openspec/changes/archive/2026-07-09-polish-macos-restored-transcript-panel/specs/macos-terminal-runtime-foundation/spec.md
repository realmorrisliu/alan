## MODIFIED Requirements

### Requirement: Restored transcript history seeds newly created terminal runtimes
The terminal runtime service SHALL accept restored transcript history when a
terminal ContentInstance is materialized from a workspace manifest with a
terminal transcript snapshot and SHALL keep that restored state clearable by
terminal content identity before treating it as durable runtime state.

#### Scenario: Runtime starts with restored transcript
- **WHEN** a restored terminal ContentInstance has a terminal transcript snapshot
- **THEN** the runtime service receives the transcript for the new runtime or restored-cache path
- **AND** the new shell starts in the restored cwd and can accept subsequent input
- **AND** the UI may render the restored transcript as a distinct restored-context panel instead of replaying it into the live PTY

#### Scenario: Restored alternate-screen snapshot
- **WHEN** a transcript snapshot was captured from alternate-screen terminal mode
- **THEN** Alan records the captured mode metadata
- **AND** the restored runtime or restored-context panel may present the captured text without claiming that the prior alternate-screen application is still running

#### Scenario: Restored transcript cache is evicted
- **WHEN** the shell host clears the restored transcript for a terminal ContentInstance
- **THEN** the runtime registry and runtime service evict any restored transcript cache for that content ID
- **AND** an existing or future surface handle for the same content ID does not reseed itself from the cleared transcript

#### Scenario: Snapshot metadata is debug-only
- **WHEN** a runtime or terminal content is associated with a restored transcript snapshot
- **THEN** debug or control-plane metadata may indicate that the content was restored from a snapshot
- **AND** the default terminal UI only shows user-facing restored-session chrome as the quiet restored-context panel defined by the shell UI contract
