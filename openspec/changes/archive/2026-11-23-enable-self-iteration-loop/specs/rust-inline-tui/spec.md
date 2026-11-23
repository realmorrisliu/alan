## ADDED Requirements

### Requirement: Sanitized GitHub issue intake slash command
The file-backed TUI SHALL provide `/issue` (with alias `/bug`) to draft a bug
report or enhancement request from strictly redacted session diagnostics,
require explicit user review and confirmation before any outbound action, and
submit through mounted `/mnt/github`, authenticated `gh` CLI, or a prefilled
GitHub web URL fallback.

#### Scenario: Drafting an issue includes redacted diagnostics and waits for confirmation
- **WHEN** the operator invokes `/issue` or `/bug` in the TUI
- **THEN** the TUI constructs an editable draft containing only redacted runtime metadata (Alan version, OS/architecture, active provider/model label, permission posture, and latest error or notice summary)
- **AND** it displays the draft for confirmation without sending any network request automatically

#### Scenario: Confirmed submission uses mounted githubfs, gh CLI, or web URL fallback
- **WHEN** the operator confirms the drafted issue
- **THEN** the TUI writes to `/mnt/github/repos/<owner>/<repo>/issues/new` if writable in the session namespace
- **AND** otherwise invokes authenticated `gh issue create` when available or presents a bounded prefilled GitHub `issues/new` web URL when neither `/mnt/github` nor authenticated `gh` is available

#### Scenario: Cancelling issue intake sends nothing
- **WHEN** the operator cancels the `/issue` confirmation prompt
- **THEN** no file write, CLI subprocess, or outbound network action occurs
- **AND** the active session returns cleanly to normal input mode

#### Scenario: Secrets and unapproved transcript content are excluded
- **WHEN** the TUI assembles the default `/issue` diagnostic payload during a session containing project files, environment secrets, or prior conversation messages
- **THEN** full conversation transcripts, project source contents, environment variables, and credentials MUST NOT be included automatically in the draft
