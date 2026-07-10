## ADDED Requirements

### Requirement: Alan Voice brand and scope
Alan SHALL present macOS voice input as **Alan Voice**, with **Hold to Talk** as
the first-phase interaction. User-visible copy SHALL use canonical Alan casing
and SHALL NOT describe the feature as dictation, always listening, a voice call,
or a Siri-style assistant.

#### Scenario: User sees voice entry points
- **WHEN** menus, settings, shortcuts, or capture feedback name the feature
- **THEN** they use `Alan Voice` and `Hold to Talk`
- **AND** technical recognizer/provider labels remain in explicit diagnostics

### Requirement: Voice Service is a host-backed file server
Alan Voice SHALL expose capture and recognition through an aP service posted at
`/srv/voice` and mounted at `/mnt/voice`. Apple Speech, audio APIs, cloud SDKs,
and secret storage SHALL remain behind the adapter and SHALL NOT be required by
Alan Kernel or Agent Processes.

#### Scenario: Voice Service starts
- **WHEN** Alan for macOS enables the service
- **THEN** the access-filtered handle and mounted tree expose config, status,
  capture records, drafts, results, and events
- **AND** no daemon session or typed RPC is required

### Requirement: Hold to Talk uses a capture lifecycle object
Alan Voice SHALL allocate a capture object for each Hold to Talk interaction and
control start, stop, cancel, and reviewed commit through its owning `ctl`.

#### Scenario: User holds and releases
- **WHEN** the user presses and holds the configured shortcut, then releases it
- **THEN** capture status moves from recording to recognition through ordered
  files/events
- **AND** overlapping capture is not started silently

#### Scenario: User cancels
- **WHEN** the user presses Escape during recording, recognition, or review
- **THEN** capture becomes terminally cancelled
- **AND** no transcript, task, app mutation, Tool, or Agent Process submission is
  committed from that capture

### Requirement: Local recognition is default and does not upload audio
Alan Voice SHALL use Local Mode by default and SHALL avoid cloud audio upload
when Local Mode is selected. It SHALL report unavailability rather than silently
switching modes.

#### Scenario: Local recognition is available
- **WHEN** the platform supports on-device recognition for the selected locale
- **THEN** captured audio is recognized locally and transcript/intent files are
  produced without cloud audio upload

#### Scenario: Local recognition is unavailable
- **WHEN** local recognition cannot run
- **THEN** status/result explain the unavailable condition
- **AND** audio is not uploaded unless the user explicitly switches to Cloud Mode

### Requirement: Cloud recognition is explicit
Cloud Mode SHALL require explicit provider selection, available host-managed
credentials, and visible audio-upload disclosure before capture.

#### Scenario: Cloud credentials are missing
- **WHEN** Cloud Mode is selected without a usable credential reference
- **THEN** capture does not upload audio
- **AND** Alan offers credential repair or a return to Local Mode

### Requirement: VoiceIntent is a reviewable domain document
Recognition SHALL produce a typed intent proposal containing transcript,
normalized text, intent kind, target descriptor or namespace path, confidence,
safety class, proposed operation, and review state. It SHALL NOT define a Kernel
intent type or global command registry.

#### Scenario: Intent is ambiguous or state-changing
- **WHEN** confidence is low, the target is unclear, or the operation changes
  state
- **THEN** the proposal remains a draft or capture-owned review record
- **AND** no mutation occurs before review and current target authorization

### Requirement: First-phase intents resolve to native operations
Alan Voice SHALL support capture, agent request, task, search, and app-command
intents by resolving them to authorized file writes, owning `ctl` writes, `/bin`
Tool execution, or bounded Agent Executable spawn.

#### Scenario: Agent request targets an existing agent
- **WHEN** the focused surface supplies an authorized Agent Process descriptor
- **THEN** Alan Voice writes the accepted request to that process's `io/input`
- **AND** it does not address a daemon session id

#### Scenario: Agent request starts new work
- **WHEN** no existing agent is selected and the user commits the request
- **THEN** the app opens bounded context descriptors and spawns an Agent
  Executable
- **AND** the new Process is visible in `/proc` and `/agent`

#### Scenario: Target app service is unavailable
- **WHEN** a task, search, capture, or app-command target is not mounted or lacks
  required rights
- **THEN** the intent stays a safe draft or asks for target selection
- **AND** no global object id or host callback bypasses the namespace

### Requirement: Feedback is compact and keyboard-first
Alan for macOS SHALL show compact recording, recognizing, review, success,
cancelled, unavailable, and error states while keeping the active terminal or
app content visible. The primary flow SHALL be operable without a pointer.

#### Scenario: Recognition takes noticeable time
- **WHEN** output is not ready within the fast feedback window
- **THEN** the overlay reflects current capture-file status and remains
  cancellable by keyboard

### Requirement: Permissions and privacy are repairable
Alan Voice SHALL explain and provide repair paths for microphone, speech
recognition, global shortcut, and any required accessibility permissions. It
SHALL show the active recognition mode and cloud provider before cloud upload.

#### Scenario: Permission is revoked
- **WHEN** a required permission is unavailable
- **THEN** capture status reports the specific missing permission and a repair
  path
- **AND** the service does not remain falsely in recording state

### Requirement: Voice initialization is off the startup critical path
Alan Voice SHALL initialize heavyweight audio, recognition, and cloud clients
lazily when invoked or configured.

#### Scenario: Alan for macOS starts
- **WHEN** Alan Voice is enabled but unused
- **THEN** app, terminal, and current-content startup do not wait for recognizer
  or cloud-provider initialization

### Requirement: Legacy fixed-command voice control is retired
Alan Voice SHALL remove the old `NSSpeechRecognizer` fixed-command controller as
a parallel user-facing path. Former command phrases SHALL pass through the same
intent proposal and file/process execution rules as other speech.

#### Scenario: A former fixed phrase is spoken
- **WHEN** recognition produces text matching the old vocabulary
- **THEN** Alan Voice resolves it as a normal app-command proposal
- **AND** unsupported commands fail through the same recoverable result path

### Requirement: Host compatibility bridge is deletion-bound
The system SHALL name any temporary Alan for macOS callback bridge
`AlanVoiceHostCompatibilityBridge`, translate it to canonical Voice Service file
operations, keep intent truth outside the bridge, add no bridge-only behavior,
and delete it when the host consumes aP directly.

#### Scenario: Host bridge is inspected
- **WHEN** maintainers audit the Voice implementation
- **THEN** the bridge's consumer, translated files, and deletion gate are explicit
- **AND** remote or Agent Process clients do not depend on the bridge
