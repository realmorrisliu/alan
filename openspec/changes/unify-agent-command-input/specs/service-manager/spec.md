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
`never`: an unexpected exit SHALL be reported to the owning application, not
silently replaced by a fresh or automatically recovered Machine. Other service
restart policies SHALL NOT revive an exited foreground application.

#### Scenario: Root Agent crash loops
- **WHEN** the foreground Root Agent fails after readiness
- **THEN** its Process failure is observable and the application reports the failure
- **AND** Service Manager does not automatically start a replacement Root Agent
- **AND** resuming durable work requires explicit recovery selection

#### Scenario: Another required service exhausts its restart budget
- **WHEN** a required non-Root service exceeds its configured retry budget
- **THEN** boot fails before readiness or the running instance becomes degraded
- **AND** retries remain bounded within that invocation
