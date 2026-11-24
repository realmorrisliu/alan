# Spec Delta

## ADDED Requirements

### Requirement: Single-viewport progressive disclosure for workstreams and focus switching
The Rust inline TUI SHALL preserve its single-viewport, scrollback-safe POSIX/VT layout without adding a permanent sidebar or workspace manager. It SHALL display calm inline receipts whenever Root routes or hands off work, show compact active/idle/inbox counts in the status bar, provide `/streams` for bounded on-demand workstream inspection and management, and provide `/focus <name>` and `/root` to switch the non-owning TUI file client between `/agent/<child_pid>` and `/agent/root`.

#### Scenario: Inline routing receipt and compact status badge appear without sidebar
- **WHEN** Root dispatches a request to a named workstream or processes a `routefs` handoff
- **THEN** the transcript renders a concise inline routing receipt and the status bar updates compact stream and inbox counts
- **AND** no permanent sidebar or full-screen panel is added

#### Scenario: User switches focus between Root and a child workstream
- **WHEN** the user runs `/focus <name>` and later `/root`
- **THEN** the non-owning TUI switches its mounted target between `/agent/<child_pid>` and `/agent/root`
- **AND** terminal scrollback and narrow-pane resize reflow remain intact
