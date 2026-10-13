# tool-result-presentation Specification

## Purpose
Defines tool-agnostic result presentation primitives, human-readable Tool call
titles, additive structured completion payloads, and consistent TUI rendering.

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

A Tool-call-started record SHALL carry an optional human-readable title formatted by the Agent Execution Engine, and renderer hosts SHALL display that title without interpreting Tool arguments.

#### Scenario: Title is shown verbatim

- **WHEN** a Tool call starts with a title such as `Read src/foo.rs` or `Bash cargo test`
- **THEN** the renderer displays that title as the Tool header
- **AND** it does not parse the Tool's argument schema to build the header

#### Scenario: Missing title degrades to Tool name

- **WHEN** a Tool call starts without a title
- **THEN** the renderer displays the Tool name

### Requirement: Structured completion payload is additive and backward-compatible
Tool-call-completed records SHALL carry optional structured presentation while
retaining fallback text. The production file-backed path SHALL carry the same
runtime-owned title, presentation and fallback preview through additive metadata
in `actions/<id>/result`, independently of retained raw `output`. Renderer hosts
SHALL decode these generic forms without interpreting Tool arguments. Metadata
SHALL pass through the existing durable-evidence redaction boundary and be
persisted before terminal Action status is published.

#### Scenario: Structured payload is preferred when present
- **WHEN** a completed Action includes a structured presentation
- **THEN** the file-backed TUI renders that presentation and its runtime title
- **AND** user commands and Agent Tool calls use the same projection boundary

#### Scenario: Fallback when payload absent
- **WHEN** an older or dynamic Action has no usable presentation metadata
- **THEN** the renderer retains a bounded textual fallback and Tool name
- **AND** it does not fabricate a successful structured result

#### Scenario: Older consumers are unaffected
- **WHEN** a consumer does not understand the additive Action result metadata
- **THEN** existing exit status, correlation and raw output remain available
- **AND** absent metadata remains compatible with older durable Action records

#### Scenario: Metadata is published atomically with completion evidence
- **WHEN** a Tool completes through the authoritative Action path
- **THEN** raw output and redacted presentation metadata are durable before its terminal status event
- **AND** recovery preserves the presentation without re-executing the Tool

### Requirement: TUI renders each presentation primitive distinctly
The TUI SHALL render each existing presentation primitive appropriately through
the production file-backed path. Routine results SHALL use bounded summaries;
the user SHALL be able to open retained detail and return to the same draft.
Summary bounds SHALL account for rendered physical rows, including long single
lines, rather than only newline counts. Raw evidence SHALL remain distinct from
its user-facing summary.

#### Scenario: Diff renders with change markers
- **WHEN** a `Diff` payload is rendered
- **THEN** its summary shows the affected path and change counts
- **AND** retained detail distinguishes additions and removals with text markers as well as optional color

#### Scenario: Command renders cmdline and exit status
- **WHEN** a `Command` payload is rendered
- **THEN** the command and exit status are visible in its summary
- **AND** available stdout and stderr remain distinguishable in details

#### Scenario: File content renders path and counts
- **WHEN** a `FileContent` payload is rendered
- **THEN** the path and available line count appear in its summary
- **AND** retained content can be inspected without rendering escaped JSON as the default result

#### Scenario: Large output collapses
- **WHEN** a payload exceeds the summary's physical-row budget, including a single long line
- **THEN** it remains bounded with a visible detail action
- **AND** closing details restores the draft and inline transcript position

#### Scenario: Host reserves ordinary page keys
- **WHEN** details are open in a terminal Host that consumes ordinary PageUp or PageDown for Host scrollback
- **THEN** unmodified Space and b also page the existing readable detail forward and backward
- **AND** the detail hint shows these usable aliases while existing page keys and Action selection remain available
- **AND** closing details restores the draft, and these aliases do not replace normal draft input outside details

#### Scenario: Details were not retained
- **WHEN** the result is truncated or its evidence is no longer available
- **THEN** the UI labels the missing portion or unavailable detail truthfully
- **AND** expanding does not fabricate content or re-execute the Tool

### Requirement: Tool failures explain an available next action
Tool failure summaries SHALL identify the failed operation, an understandable
cause and an available next action when known. Internal diagnostics SHALL remain
available in details without becoming the entire default user-facing message.

#### Scenario: Project execution lacks authorization
- **WHEN** a project Tool cannot execute because no suitable project authority is present
- **THEN** Alan explains that project access is needed and points to the authorization flow
- **AND** it does not describe the Tool or operation as successful

#### Scenario: A failure has no known remedy
- **WHEN** a Tool fails without a reliable user action that resolves it
- **THEN** Alan reports the failure and offers available diagnostic detail
- **AND** it does not invent an authorization action, retry outcome or fix

### Requirement: Routine Action summaries avoid redundant scaffolding
The TUI SHALL render bounded Action summaries using the Runtime-owned title,
existing presentation primitive and authoritative outcome. It SHALL avoid
repeating the same operation, path or command in title and summary. Missing
presentation metadata SHALL retain a readable bounded fallback without inferred
success. The existing primitive-specific information and retained-detail
requirements SHALL continue to apply.

#### Scenario: A command completes with a structured result
- **WHEN** command title and payload identify the same command
- **THEN** the routine summary presents the command once and keeps its actual exit state visible
- **AND** detail retains distinguishable stdout and stderr without re-execution

#### Scenario: A file result repeats its path
- **WHEN** the title and structured file or diff payload identify the same path
- **THEN** the routine summary shows that path once with the available content or change metadata
- **AND** the original contents or diff remain inspectable

#### Scenario: A dynamic Tool has only fallback metadata
- **WHEN** a Tool has no supported structured presentation
- **THEN** the summary uses the available human title or Tool name and bounded fallback
- **AND** the TUI does not interpret Tool arguments or call a model to invent a summary

### Requirement: Adjacent successful read-only Actions may share presentation
The TUI SHALL support compact groups of adjacent successful Actions only when
Runtime-owned semantics positively identify them as read-only and their order,
turn and authority context are correlated. Missing eligibility SHALL produce a
standalone result. Grouping MUST NOT infer eligibility from titles, result
content, a presentation primitive or an arbitrary shell command. Individual
Action identity, status and retained detail SHALL remain accessible.

#### Scenario: A sequence of eligible reads completes
- **WHEN** adjacent successful read or search Actions have eligible semantics in the same correlated turn and authority context
- **THEN** the renderer can present their uncommitted summaries as one bounded group
- **AND** the user can select and inspect each member's own result

#### Scenario: An operation ends a group
- **WHEN** a message, explicit command, plan change, request, write, failure, rejection, cancellation, unknown operation or authorization change separates eligible Actions
- **THEN** they are not grouped across that boundary
- **AND** significant outcomes and requests remain distinct

#### Scenario: Eligibility is unknown or claims come from content
- **WHEN** an older record lacks classification or Tool text claims that a call is read-only
- **THEN** the result remains standalone unless authoritative Runtime metadata independently establishes eligibility
- **AND** ordinary content never changes authority or grouping eligibility

#### Scenario: Registered semantics do not match the actual execution owner
- **WHEN** the Runtime knows a read-only native implementation but the Tool Process executed through a different runner or has no matching execution receipt
- **THEN** it does not export grouping eligibility for that Action
- **AND** missing, oversized or changed authority context and an explicit human-approval boundary also disable eligibility

#### Scenario: Completed group rows are already in native scrollback
- **WHEN** another eligible Action finishes after the earlier group's rows have been committed
- **THEN** the renderer leaves those rows intact and presents subsequent work without rewriting native scrollback
- **AND** a running Action remains visible without waiting for group completion
