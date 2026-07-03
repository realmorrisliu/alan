## ADDED Requirements

### Requirement: Editable buffers are file-server surfaces

Alan OS SHALL expose an editable interaction buffer as a file-server directory
with `body`, `tag`, `ctl`, `addr`, and `event` files.

#### Scenario: Buffer files are inspectable

- **WHEN** a client walks an editable buffer directory
- **THEN** it can list and open `body`, `tag`, `ctl`, `addr`, and `event` using
  normal aP operations

#### Scenario: Body and tag are text surfaces

- **WHEN** a client reads `body` or `tag`
- **THEN** the server returns UTF-8 text bytes representing the buffer content or
  command/status tag content

#### Scenario: Successful text writes update the surface

- **WHEN** a client successfully writes UTF-8 text to `body` or `tag`
- **THEN** subsequent reads of that file return the updated text

#### Scenario: Successful range replacement updates the body

- **WHEN** a client successfully replaces text in a `body` range
- **THEN** subsequent reads of `body` include the replacement at that range and
  no longer return the replaced bytes at that range

### Requirement: Text ranges are addressable

Alan OS SHALL represent the active text range through an `addr` file that can be
read and written by clients with write authority. The visible `addr` value SHALL
include both the selected range and the revision of that address selection.

#### Scenario: Client selects a body range

- **WHEN** a client writes a valid range expression to `addr`
- **THEN** subsequent reads of `addr` return that range with a new address
  revision and operations that consume the active range can bind to that
  revision

#### Scenario: Stale range is rejected

- **WHEN** a client commits an operation against an `addr` range whose source
  body revision is no longer current
- **THEN** the operation fails with a typed aP error and does not apply to a
  different range

#### Scenario: Stale address selection is rejected

- **WHEN** a client commits an operation against an `addr` revision or range
  that is no longer the current address selection
- **THEN** the operation fails with a typed aP error and does not consume a
  different client's selected range

### Requirement: Executable text uses explicit control operations

Alan OS SHALL execute text from an editable buffer only through explicit `ctl`
operations that resolve to normal Alan Shell, process, or routing behavior under
the caller's namespace capabilities.

#### Scenario: Selected text is executed

- **WHEN** a client writes an `exec` control operation carrying the expected
  `addr` range and address revision
- **THEN** Alan resolves the selected text through the normal shell/action/process
  path and records the execution in the buffer's `event` stream

#### Scenario: Concurrent selection changes do not retarget execution

- **WHEN** one client records an `addr` range and address revision, another
  client changes `addr`, and the first client writes `exec` for the recorded
  range and revision
- **THEN** the operation fails with a typed aP error and does not execute text
  from the other client's range

#### Scenario: Execution does not bypass authority

- **WHEN** selected text would require a capability not present in the caller's
  namespace or policy context
- **THEN** execution is denied or yields through the normal policy path rather
  than being granted by the buffer surface

### Requirement: Buffer activity is observable as events

Alan OS SHALL expose edits, address changes, and explicit executions through the
buffer `event` stream using blocking-read semantics.

#### Scenario: Edit event is observed

- **WHEN** a client writes new content to `body`
- **THEN** another client reading `event` at the live edge blocks until an edit
  event is appended and then receives that event

#### Scenario: Execution event is observed

- **WHEN** a client executes selected text through `ctl`
- **THEN** the `event` stream records the source range, command text, and
  resulting accepted, denied, yielded, or failed status

### Requirement: Editable buffers do not replace M0-M2 agent IO

Alan OS SHALL keep editable buffers as an interaction layer above append-only
agent `io/` streams; M0-M2 agent operation SHALL continue to work through `io/`
and `ctl` without requiring editable buffers.

#### Scenario: Agent IO remains sufficient

- **WHEN** an agent process is driven through its existing `io/` and `ctl` files
- **THEN** it does not require an editable buffer directory to start, receive
  input, stream output, yield, or halt
