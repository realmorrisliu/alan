## MODIFIED Requirements

### Requirement: The request is assembled from the namespace
Alan OS SHALL assemble the logical model request as a view over namespace files:
`machine/tape`, `context/`, and visible Tool packages. Tape compaction SHALL be
a view over `machine/tape` (tape is truth; the context-window view is what is
sent), not a hidden runtime step. An agent's model-callable Tools SHALL be
exactly the visible `/bin` entries that carry a valid Tool manifest at
`/lib/exec/<tool>/manifest`. Agent Executables and ordinary commands in the
`/bin` union are spawn targets, not model-callable Tools, and SHALL NOT appear in
the request's Tool list. Tool definition, capability, locality, and execution
metadata SHALL come from the mounted package files, with no separate catalog or
registry authority.

#### Scenario: Context is changed
- **WHEN** a file is bound into or removed from an agent's `context/` or `/bin`
- **THEN** the next assembled request reflects the change
- **AND** no separate prompt-assembly configuration is edited

#### Scenario: The provider is changed
- **WHEN** the provider mount is rebound to a different LLM file server
- **THEN** the agent's request assembly is unchanged
- **AND** only the provider-local wire translation differs

#### Scenario: The model's Tool list is computed
- **WHEN** the request's available Tools are computed
- **THEN** they are the visible `/bin` entries with valid manifests under
  `/lib/exec/<tool>`, excluding Agent Executables and ordinary commands
- **AND** there is no separate Tool catalog or registry granting Tools outside
  the namespace

#### Scenario: A mounted Tool package is incomplete
- **WHEN** the executable or manifest half of a Tool package is absent or the
  manifest cannot be validated
- **THEN** request assembly does not expose that entry as a model-callable Tool
- **AND** the failure identifies the incomplete mounted package rather than
  consulting process-global defaults

## ADDED Requirements

### Requirement: AgentFS is the complete observable runtime-state boundary
AgentFS SHALL own Agent Process output, tape, requests, actions, machine
snapshots, renderer-safe UI state, and ordered update streams under `/agent`,
while `/proc` SHALL own generic Process state. Hosts and supervisors SHALL NOT
require an engine handle, callback, or broadcast receiver to observe equivalent
live state.

#### Scenario: Host attaches after a turn has started
- **WHEN** a host opens an already-running Agent Process
- **THEN** it hydrates snapshots and resumes streams from `/agent/<pid>` and
  reads generic lifecycle from `/proc/<pid>`
- **AND** attachment does not depend on having received earlier in-memory events

#### Scenario: Parallel live state API is added
- **WHEN** current engine code exposes output, request, action, machine, or UI
  state through a callback or publish/subscribe channel
- **THEN** repository verification fails because the owning file surface is the
  complete observable boundary
