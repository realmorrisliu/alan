## ADDED Requirements

### Requirement: Input shadow evaluation remains Machine-owned advice
An explicitly selected shadow evaluation SHALL be consumed by the existing Agent Machine using a reachable Connection and its captured finite candidate set. It SHALL preserve original input and submission identity, deterministic prefix/response precedence and current governance. Its classification SHALL NOT dispatch command, Tool or fallback work.

#### Scenario: A command is selected in shadow mode
- **WHEN** the typed result selects the submitted command candidate
- **THEN** the Machine records advice for qualification without invoking command or Tool dispatch
- **AND** no renderer or separate router acquires execution authority

#### Scenario: Evaluation is incomplete at recovery
- **WHEN** durable evidence records an evaluation without a reliable terminal result
- **THEN** recovery exposes an interrupted or unavailable evaluation
- **AND** it does not repeat the model call, synthesize a selection or replay an effect

#### Scenario: Malformed provider evidence reaches the Machine
- **WHEN** a committed evaluation returns malformed response evidence through llmfs
- **THEN** the Machine records a malformed terminal outcome rather than provider unavailability
- **AND** recovery retains that category without repeating the evaluator or inventing usage or cost

### Requirement: Shadow observations cross acknowledged durability barriers
The Machine SHALL persist captured admission and evaluation identity before committing a model request, and persist a validated terminal outcome before publishing completed advice. It SHALL use existing rollout/checkpoint evidence, preserve the separate ordinary-input lifecycle, and never retry an uncertain or recovered operation implicitly.

#### Scenario: Persistence fails before evaluation commit
- **WHEN** started-observation persistence fails or its acknowledgement is uncertain
- **THEN** the Machine does not commit evaluation data or allocate a replacement attempt
- **AND** it reconciles the same observation without command, Tool or fallback dispatch

#### Scenario: Provider returns but terminal persistence is unconfirmed
- **WHEN** a provider result exists but terminal evidence is not reliably persisted
- **THEN** the Machine does not publish successful terminal advice
- **AND** recovery reports interrupted evidence without repeating the model request

#### Scenario: Original input travels through different admission surfaces
- **WHEN** a qualification input becomes an ordinary submission or a pending-request response
- **THEN** evidence correlates the retained original bytes with that actual submission or request identity
- **AND** it does not reconstruct raw input from intent, reinterpret response prefixes, or substitute synthetic admission for an unsupported client path

#### Scenario: Explicit intent or request response bypasses evaluation
- **WHEN** an explicitly configured Machine admits explicit command/Agent input or a current user Confirmation/StructuredInput response
- **THEN** it acknowledges one bypass observation with its reason, payload digest and zero evaluator calls before publishing it
- **AND** it allocates no evaluation operation and records no invented model usage
- **AND** request responses correlate by owning rollout and request identity across repeated deliveries and recovery, while a new rollout may reuse a request id for a distinct response
- **AND** this evidence does not grant response acceptance, settle ordinary input or dispatch effects
- **AND** unknown/stale response IDs and Host Mount Service controls do not reserve bypass identity


### Requirement: Explicit source-owner work uses ordinary input ownership
The bounded source-owner Machine program SHALL be selected only by an explicit
version-1 request. The `owner-work-v1 ` Machine control SHALL carry an `id` UUID
and `request`, containing exactly `version`, `question`, `evaluator_profile`
and `candidates`. Each candidate SHALL contain an `id` and `sources`; each
source SHALL contain a namespace `path`, `start_line` and `end_line`. The control
SHALL become a ForceAgent FollowUp submission in the existing durable input queue.
Ordinary text, JSON-looking prose and shadow advice SHALL NOT select this program.
The existing `agent_work` executable SHALL submit that control with its
`select_owner` JSON action and inspect the projection with `result`.

The Machine SHALL validate version 1, nonempty question of at most 8192 UTF-8
bytes, at most 16 unique component IDs, one to eight source ranges per candidate,
a control document of at most 64 KiB, absolute normalized `/mnt/` namespace paths
of at most 4096 bytes, and positive inclusive ranges of at most 1000 lines.
Source reads SHALL reuse ordinary Tool resolution, policy and Process execution;
a missing or revoked descriptor SHALL remain unavailable. Captured source content SHALL total at most 128 KiB. Each candidate's serialized
citation descriptors SHALL occupy at most 4096 bytes. Candidate projection
SHALL be limited to 4096 UTF-8 bytes with explicit truncation; completion SHALL
retain the full range digest and never cite absent lines.

#### Scenario: An explicit request joins the existing queue
- **WHEN** an authorized client submits a valid source-owner control
- **THEN** its UUID and typed payload follow ordinary ordered admission and recovery
- **AND** no renderer, global router or new Kernel Process kind executes the work

#### Scenario: Prose resembles a work control
- **WHEN** ordinary input contains JSON resembling an owner request
- **THEN** it remains ordinary Agent input and cannot bypass explicit admission

#### Scenario: Evidence authority is unavailable
- **WHEN** a candidate range cannot be read under the Agent's current authority
- **THEN** the work records unavailable failure without an evaluator or fallback call
- **AND** supplied paths and model confidence grant no authority

### Requirement: Source-owner work advances with bounded durable decisions
The Machine SHALL acknowledge a work start before source dispatch and each model
start before model commit. Exact unique line-oriented public Rust struct, enum or trait declaration name
ownership in the supplied evidence MAY complete the literal `Which crate defines
NAME?` question without a model call. Other questions SHALL use at most one
`choice.v1` evaluation through a separately captured Connection explicitly granted
at Process launch; a request's profile name SHALL NOT acquire Connection authority.
The current Root host selection MAY share its explicitly captured shadow evaluator
with this explicit program; shadow advice itself SHALL remain unable to select work.
Children SHALL receive no implicit evaluator grant. Selection SHALL
be validated against captured candidates and retained source ranges.

Only NoMatch MAY enter at most one generation fallback, with a 30,000 ms phase
deadline and a verified pre-dispatch maximum cost of 1000 micro-USD or less.
Quote provenance SHALL be nonempty and occupy at most 1024 UTF-8 bytes.
Unknown billing SHALL prevent fallback dispatch. Expiry SHALL request abort through
existing generation control; an uncertain terminal/abort result SHALL remain
unsettled for interrupted recovery without another dispatch. Other evaluation failures SHALL
remain typed failures. Unresolved choice SHALL wait through the existing owned
StructuredInput request. An explicit valid candidate response SHALL complete the
same work after current source authority and digest validation; it SHALL NOT
repeat evaluation or replenish attempts. Cancellation SHALL prevent further
model or effect dispatch. Recovery SHALL retain terminal evidence and owned waits;
unsettled active work SHALL be interrupted rather than retried. Unknown Tool
effects SHALL retain the existing reconciliation contract.

#### Scenario: Structured work waits and resumes
- **WHEN** NoMatch cannot enter a budget-qualified generation fallback
- **THEN** a durable wait references the existing StructuredInput request
- **AND** an accepted response resumes that work identity with its spent attempt budget

#### Scenario: A newly created request lacks acknowledged wait ownership
- **WHEN** AgentFS creates the request but work-wait persistence fails
- **THEN** the Machine cancels that same request and verifies its terminal status before returning failure
- **AND** it does not register or yield an unacknowledged wait, retry the evaluator or invent successful cleanup

#### Scenario: Waiting bytes survive an unacknowledged write
- **WHEN** recovery finds Waiting evidence without its matching post-acknowledgement witness
- **THEN** work becomes Interrupted with retained request identity and spent attempts
- **AND** no answerable request is restored from that unacknowledged wait
- **AND** the witness is queued only after wait persistence returns acknowledgement, through the same rollout owner and before pending-request registration

#### Scenario: A work start lacks a terminal acknowledgement
- **WHEN** explicit recovery finds active work without an acknowledged result
- **THEN** it projects interrupted work without repeating source or model dispatch

#### Scenario: A selected member completes
- **WHEN** the selection names a captured candidate with validated retained evidence
- **THEN** work completes with that owner and source citations without final prose generation
- **AND** the same Agent Process may accept a later task
