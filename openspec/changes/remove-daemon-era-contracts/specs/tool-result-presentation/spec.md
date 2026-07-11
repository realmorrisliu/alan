## MODIFIED Requirements

### Requirement: Tool calls carry a human title at start

A Tool-call-started record SHALL carry an optional human-readable title formatted by the Agent Execution Engine, and renderer hosts SHALL display that title without interpreting Tool arguments.

#### Scenario: Title is shown verbatim

- **WHEN** a Tool call starts with a title such as `Read src/foo.rs` or `Bash cargo test`
- **THEN** the renderer displays that title as the Tool header
- **AND** it does not parse the Tool's argument schema to build the header

#### Scenario: Missing title degrades to Tool name

- **WHEN** a Tool call starts without a title
- **THEN** the renderer displays the Tool name
