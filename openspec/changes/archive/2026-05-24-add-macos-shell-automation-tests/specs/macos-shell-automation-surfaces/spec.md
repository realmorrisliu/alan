## ADDED Requirements

### Requirement: Shell entities are available to App Intents
alan's macOS app SHALL expose App Entity representations for shell windows,
spaces, tabs, panes, and attention items using stable identifiers and
user-facing display names.

#### Scenario: Query active tabs
- **WHEN** an App Intent query asks for tabs in the active shell window
- **THEN** alan returns tab entities with user-facing titles, space context, and stable internal identifiers

#### Scenario: Query panes with secure input
- **WHEN** an App Intent query includes a pane whose terminal is in secure-input state
- **THEN** alan returns safe metadata and does not expose sensitive terminal input content

#### Scenario: No shell window available
- **WHEN** App Intent resolution runs while no shell window is active
- **THEN** alan returns an empty or needs-app state instead of crashing or fabricating entities

### Requirement: Core shell actions have App Intents
alan's macOS app SHALL provide App Intents for creating terminal tabs, creating
alan tabs, splitting panes, focusing panes, closing panes or tabs, sending text,
reading pane summaries, and opening attention items.

#### Scenario: Create terminal tab intent
- **WHEN** the user runs the create terminal tab intent
- **THEN** alan creates a terminal tab through the shell controller and returns the created tab summary

#### Scenario: Split pane intent
- **WHEN** the user runs a split pane intent with a direction and target pane
- **THEN** alan performs the same split mutation as the native command path and returns the resulting focused pane

#### Scenario: Send text intent
- **WHEN** the user runs a send text intent for a target pane
- **THEN** alan routes delivery through the terminal runtime service and reports accepted, queued, or rejected state truthfully

#### Scenario: Open attention item intent
- **WHEN** the user runs an intent for an attention item
- **THEN** alan activates the owning window, space, tab, and pane without exposing raw debug identifiers in the result text

### Requirement: App Intent outcomes align with shell commands
App Intent handlers SHALL use the same shell controller mutations and result
semantics as native commands and control-plane operations unless a documented
availability or privacy restriction applies.

#### Scenario: Missing target
- **WHEN** an intent targets a missing pane or tab
- **THEN** it returns the same stable missing-target category used by the shell command/control-plane path

#### Scenario: Runtime unavailable
- **WHEN** an intent depends on terminal runtime availability and the runtime service reports non-ready state
- **THEN** the intent returns a user-facing failure result aligned with control-plane runtime status

#### Scenario: Command succeeds
- **WHEN** an intent completes a shell mutation
- **THEN** shell state, event stream, and intent result reflect the same accepted mutation

### Requirement: Automation respects terminal privacy
Automation surfaces SHALL avoid exposing raw terminal content, secure input,
private socket paths, raw pane IDs, or debug payloads unless the user explicitly
opens a debug context outside App Intents.

#### Scenario: Read pane summary
- **WHEN** an App Intent reads a pane summary
- **THEN** the summary includes safe metadata such as title, cwd, process status, and attention state, but not arbitrary terminal buffer text

#### Scenario: Secure input active
- **WHEN** a pane is in secure-input state
- **THEN** automation summaries redact sensitive fields and command results do not echo submitted secret text
