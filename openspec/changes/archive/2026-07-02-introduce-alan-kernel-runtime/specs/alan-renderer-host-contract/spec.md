## ADDED Requirements

### Requirement: Renderer hosts read files; the semantic-snapshot model is retired
This change SHALL NOT define a renderer-host contract built on pulling
versioned "semantic view snapshots" / built-in view models (conversation, form,
task tree, command palette, diff, object list, log stream) from a core. That
model is the retired ViewModel/Object/Command ontology (ADR-0024). In the Plan 9
model a renderer host is a client that reads files and writes `ctl`: it renders
from `/proc` and `/agent` files (`io/output`, `requests/<id>/`, `actions/<id>/`,
`machine/`) and translates host input into file writes and `ctl` commands.

A future, separately specified contract MAY define how a host renders structured
file surfaces (for example rendering a `requests/<id>/` tree as a form), but it
SHALL be grounded in reading files, not in consuming core-owned semantic
snapshots.

#### Scenario: A reader looks here for the renderer-host contract
- **WHEN** a reader opens this spec for the renderer-host model
- **THEN** it is directed to the file-reading client model in
  `define-plan9-kernel-substrate` and `define-agent-file-layout-contract`
- **AND** no "semantic view snapshot" pull contract is defined here

#### Scenario: The existing TUI keeps working during migration
- **WHEN** the current Ratatui Alan Shell path runs during migration
- **THEN** it remains on the existing compatibility transport until it reads the
  agent file surfaces directly
- **AND** its eventual target is reading `/proc` / `/agent` files and writing
  `ctl`, not pulling semantic snapshots from a core
