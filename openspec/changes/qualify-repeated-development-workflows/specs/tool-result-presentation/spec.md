## MODIFIED Requirements

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

#### Scenario: Acquired retained Command exceeds its preview
- **WHEN** original retained Command JSON has acquired stdout, stderr and an exit code beyond the bounded presentation preview
- **THEN** details show the acquired streams as readable text with separate stdout/stderr labels and exit status
- **AND** raw bytes remain separately available; literal role-like content and comparisons are preserved

#### Scenario: Acquired output is not a complete Command shape
- **WHEN** retained JSON lacks the generic Command fields or contains incompatible field types
- **THEN** the renderer preserves its existing fallback and truthful bound/loss notices
- **AND** it does not infer a Command from a Tool name or arguments
