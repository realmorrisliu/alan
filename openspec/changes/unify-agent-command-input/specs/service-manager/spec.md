## MODIFIED Requirements

### Requirement: Service Manager is the first system Process
The foreground alan9 composition SHALL create Kernel and start one Service
Manager Process per invocation. Service Manager SHALL be the sole owner of later
system service and Root Agent Process lifecycle; the renderer and Host adapters
MUST NOT retain fallback boot or supervision. File-Server Services SHALL NOT require
separate operating-system background processes.

#### Scenario: System boot begins
- **WHEN** an invocation's Kernel is ready
- **THEN** its composition starts Service Manager
- **AND** all other required system Processes are started by that Service Manager

### Requirement: Restart policy is bounded
Service Manager SHALL support only `never`, `on-failure`, and `always`, with
bounded exponential backoff, restart budget, and stable reset window. Required
budget exhaustion before ready SHALL fail boot; afterward it SHALL mark the
system degraded and await explicit retry. The foreground Root Agent SHALL use
`always` only within its live invocation. A replacement SHALL recover the latest
rollout path held by that instance's Agent Runtime Service, and MUST NOT reread
the channel-wide selector. A new invocation SHALL read that selector only when
the user explicitly requests `alan --resume`; no background Host survives the
foreground invocation.

#### Scenario: Root Agent crash loops
- **WHEN** the foreground Root Agent fails after readiness
- **THEN** Service Manager replaces it from that invocation's latest durable
  rollout while restart attempts remain within budget
- **AND** failed recovery leaves the selected durable evidence available for
  retry or explicit recovery

#### Scenario: Root Agent restarts while another invocation is active
- **WHEN** the foreground Root Agent fails after readiness while another
  invocation has published a different channel-wide rollout
- **THEN** Service Manager replaces it from its own invocation's latest durable
  rollout
- **AND** its recovery is independent of the other invocation's selector write
- **AND** each replacement remains subject to the bounded restart budget

#### Scenario: Another required service exhausts its restart budget
- **WHEN** a required non-Root service exceeds its configured retry budget
- **THEN** boot fails before readiness or the running instance becomes degraded
- **AND** retries remain bounded within that invocation
