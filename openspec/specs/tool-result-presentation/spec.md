# tool-result-presentation Specification

## Purpose
TBD - created by archiving change add-structured-tool-rendering. Update Purpose after archive.
## Requirements
### Requirement: Tool results are modeled by presentation form
The protocol SHALL represent rich tool results using a closed set of presentation primitives that is independent of tool identity: `Diff`, `FileContent`, `Command`, `Listing`, and `PlainText`.

#### Scenario: Built-in tool maps to a primitive
- **WHEN** a built-in tool completes (e.g. an edit, a read, a shell command, a search)
- **THEN** the runtime emits a presentation payload of the primitive that fits the result (diff, file content, command, or listing respectively)

#### Scenario: Unknown or dynamic tool falls back
- **WHEN** a dynamic, client-provided, or MCP tool completes without a fitting primitive
- **THEN** the runtime emits a `PlainText` presentation payload
- **AND** the rendering pipeline does not require a tool-specific renderer

#### Scenario: Presentation set is bounded
- **WHEN** any tool result is presented
- **THEN** it uses exactly one of the defined primitives and no tool-identity-specific protocol variant is required

### Requirement: Tool calls carry a human title at start
A tool-call-started event SHALL carry an optional human-readable title formatted by the runtime/daemon, and the TUI SHALL render that title without interpreting tool arguments.

#### Scenario: Title is shown verbatim
- **WHEN** a tool call starts with a title such as `Read src/foo.rs` or `Bash cargo test`
- **THEN** the TUI displays that title as the tool header
- **AND** the TUI does not parse the tool's argument schema to build the header

#### Scenario: Missing title degrades to tool name
- **WHEN** a tool call starts without a title
- **THEN** the TUI displays the tool name

### Requirement: Structured completion payload is additive and backward-compatible
A tool-call-completed event SHALL carry an optional structured presentation payload while retaining the flat preview string as a fallback.

#### Scenario: Structured payload is preferred when present
- **WHEN** a completed tool call includes a presentation payload
- **THEN** the TUI renders the structured payload rather than the flat preview

#### Scenario: Fallback when payload absent
- **WHEN** a completed tool call has no presentation payload
- **THEN** the TUI renders the flat preview string

#### Scenario: Older consumers are unaffected
- **WHEN** a consumer that does not understand the new fields receives the event
- **THEN** the new fields are absent-by-default in serialization and the consumer continues to function

### Requirement: TUI renders each presentation primitive distinctly
The TUI SHALL render each presentation primitive with a form appropriate to its content and SHALL collapse large outputs with an expand affordance.

#### Scenario: Diff renders with change markers
- **WHEN** a `Diff` payload is rendered
- **THEN** added and removed lines are visually distinguished and the affected path is shown

#### Scenario: Command renders cmdline and exit status
- **WHEN** a `Command` payload is rendered
- **THEN** the command line, exit code, and output streams are shown

#### Scenario: File content renders path and counts
- **WHEN** a `FileContent` payload is rendered
- **THEN** the path and line count are shown, with truncation indicated when the content was truncated

#### Scenario: Large output collapses
- **WHEN** a rendered payload exceeds the display threshold
- **THEN** the TUI shows a truncated view with a total count and an expand affordance

