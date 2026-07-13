## ADDED Requirements

### Requirement: Alan for macOS attaches to the channel system Host
Alan for macOS SHALL connect to the matching stable/dev Alan OS Host over its
protected aP endpoint and SHALL obtain a Shell Process through Local Entry
Service. It MUST NOT boot Kernel or Agent Execution Engine inside the app.

#### Scenario: App launches while Host already runs
- **WHEN** channel and readiness validation succeed
- **THEN** the app attaches without restarting Alan OS or its Processes

### Requirement: Agent ContentInstance stores an Agent Attachment
An Agent ContentInstance SHALL persist only Process Reference (boot ID and PID),
caller-held stream offsets, and host presentation. It MUST NOT persist Agent
Machine, Tape, request, Tool, provider, Process status, or socket authority.

#### Scenario: Agent view is restored
- **WHEN** the saved boot ID matches and `/proc/<pid>` exists
- **THEN** macOS reopens `/agent/<pid>` streams at saved offsets
- **AND** no Agent Process is created

### Requirement: Reattachment validates Process identity
Before reading AgentFS, macOS SHALL verify the Alan OS boot identity and
Process reference. A Host restart or missing Process SHALL produce an
unavailable/terminal view and MUST NOT attach a reused PID.

#### Scenario: PID is reused after Host restart
- **WHEN** the current boot ID differs from the saved reference
- **THEN** macOS rejects the attachment regardless of PID equality

### Requirement: Closing a view only detaches
Closing Agent content, Pane, Tab, window, or app SHALL release renderer fids and
MUST NOT terminate the Agent Process. Stop SHALL be a separate explicit write
to `/proc/<pid>/ctl`.

#### Scenario: Last visible Agent view closes
- **WHEN** no macOS ContentInstance remains visible for the Process
- **THEN** its `/proc` lifecycle is unchanged

### Requirement: Native adapters answer service requests
macOS directory authorization and connection login/Keychain adapters SHALL
answer Host Mount Service and Connection Service request files. They MUST NOT
own grants, profiles, or expose raw paths/secrets into Alan OS.

#### Scenario: User approves a directory picker
- **WHEN** the native adapter obtains platform authorization
- **THEN** it returns a bounded hostfs export result to Host Mount Service
