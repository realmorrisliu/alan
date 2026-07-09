## ADDED Requirements

### Requirement: Restored transcript panel uses terminal-aligned presentation
The macOS shell SHALL ensure any restored terminal transcript panel above the
live terminal visually aligns with the terminal surface and remains quiet,
bounded, and clearable.

#### Scenario: Restored panel text aligns with terminal text
- **WHEN** a terminal pane renders restored transcript context above the live terminal
- **THEN** restored transcript text uses terminal-like monospace typography, row height, foreground treatment, and horizontal scrolling behavior
- **AND** the restored text leading edge aligns with the live terminal text column as closely as the current terminal host composition permits
- **AND** restored transcript text uses full-width leading layout rather than centering a narrow text block in the panel

#### Scenario: Restored panel remains visually distinct
- **WHEN** restored transcript context is visible
- **THEN** Alan may use a quiet background difference and subtle separator to distinguish the prior-session context from the live terminal
- **AND** the panel does not appear as a warning banner, diagnostic card, or prominent debug surface
- **AND** the panel height remains bounded and stable for the restored transcript row limit used by the view

#### Scenario: Restored panel clears with terminal clear intent
- **WHEN** the user clears the focused terminal through supported terminal or Alan clear actions
- **THEN** the restored transcript panel disappears for that terminal content
- **AND** the live terminal still receives the clear behavior appropriate to the triggering action
