## ADDED Requirements

### Requirement: Shell core owns the workspace home default content
The shell core SHALL define a platform-neutral workspace home content kind —
the agents, recent work, and installed-services surface defined by
`alan-interaction-model` — and SHALL construct the default manifest's initial
selected Tab with that content kind. Default manifest creation SHALL NOT
hard-code a terminal Tab as the initial content; terminal Tabs SHALL be
created only through explicit user or platform actions. Platform adapters
SHALL render the workspace home content kind from mounted Alan OS file state
and SHALL NOT reimplement default-manifest selection outside shell core.

#### Scenario: Default manifest is created
- **WHEN** shell core creates a default workspace manifest
- **THEN** the initial selected Tab carries the workspace home content kind
- **AND** no terminal Tab is constructed as a default side effect

#### Scenario: An adapter renders the workspace home tab
- **WHEN** a platform adapter materializes a Tab with the workspace home
  content kind
- **THEN** it renders active agents, recent work, and installed services from
  mounted file state
- **AND** it does not substitute a terminal surface or a platform-private
  default
