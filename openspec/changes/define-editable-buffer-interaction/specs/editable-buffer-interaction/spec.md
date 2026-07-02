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

### Requirement: Text ranges are addressable

Alan OS SHALL represent the active text range through an `addr` file that can be
read and written by clients with write authority.

#### Scenario: Client selects a body range

- **WHEN** a client writes a valid range expression to `addr`
- **THEN** subsequent reads of `addr` return that range and operations that
  consume the active range use it

#### Scenario: Stale range is rejected

- **WHEN** a client commits an operation against an `addr` range whose source
  body revision is no longer current
- **THEN** the operation fails with a typed aP error and does not apply to a
  different range

### Requirement: Executable text uses explicit control operations

Alan OS SHALL execute text from an editable buffer only through explicit `ctl`
operations that resolve to normal Alan Shell, process, or routing behavior under
the caller's namespace capabilities.

#### Scenario: Selected text is executed

- **WHEN** a client writes an `exec` control operation for the active `addr`
  range
- **THEN** Alan resolves the selected text through the normal shell/action/process
  path and records the execution in the buffer's `event` stream

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
