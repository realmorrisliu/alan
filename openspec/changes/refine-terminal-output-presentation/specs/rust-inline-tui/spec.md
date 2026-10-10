## ADDED Requirements

### Requirement: Terminal presentation has no renderer-generated role prefixes
The TUI SHALL present semantic actions, content and authoritative states without
generating role labels of the form `xxx>`. This requirement SHALL cover transcript,
live status, pending requests, errors, expanded details and re-rendered retained
history. It MUST NOT remove or rewrite identical literal text from user input,
Agent answers, code, Markdown quotations or Tool output. Input route markers and
request-response semantics SHALL retain their existing behavior.

#### Scenario: Every semantic output surface uses the new presentation
- **WHEN** Alan renders a Tool, plan, pending input, error, thinking detail or another role-bearing UI element
- **THEN** it does not add a role name followed by `>`
- **AND** meaningful operations, statuses, request choices and error information remain distinguishable

#### Scenario: Literal content resembles a former prefix
- **WHEN** input, an answer, code or Tool content contains `tool>`, `server> ready`, `a > b` or a Markdown quotation
- **THEN** the renderer preserves that source content under its ordinary content-rendering rules
- **AND** it does not apply a global prefix-removal pass

#### Scenario: Older history is rendered
- **WHEN** retained structured history can be rendered again
- **THEN** newly generated UI uses the new presentation without rewriting durable evidence
- **AND** opaque historical text and already committed native scrollback are not guessed at or rewritten

### Requirement: Output hierarchy communicates actions and states
The TUI SHALL distinguish answer content, routine activity, requests and failures
through concrete wording and restrained visual hierarchy. It SHALL keep normal
activity compact and preserve authoritative failed, rejected, cancelled, waiting
and unknown states. Color or icons alone MUST NOT convey essential outcomes.
Detail discovery SHALL remain visible in the applicable context without
repeating an identical hint on every routine Action.

#### Scenario: Routine work precedes an answer
- **WHEN** several Tools complete before an Agent answer
- **THEN** their summaries omit redundant titles, paths and repeated detail-hint rows
- **AND** the answer remains visually distinct from routine activity

#### Scenario: Attention is required
- **WHEN** a request, failure or unknown outcome is displayed
- **THEN** its actual state and available user action are explicit
- **AND** it is not hidden inside a successful activity summary or converted into a success

## MODIFIED Requirements

### Requirement: Renderer file updates are classified into display tiers

The TUI SHALL classify each renderer-visible file update as permanent transcript
content, ephemeral live-region status, or suppressed lifecycle detail. A plan
change SHALL retain a compact permanent record and access to its corresponding
full snapshot rather than repeatedly printing the complete snapshot inline.

#### Scenario: Machine hydration is suppressed

- **WHEN** the renderer hydrates Agent Machine state or observes Process attachment lifecycle metadata
- **THEN** it does not print that lifecycle detail into the transcript
- **AND** it MAY retain the detail in tracing output

#### Scenario: Conversational substance is permanent

- **WHEN** AgentFS surfaces user input, assistant output, a completed Tool result, a plan snapshot, or a fatal error
- **THEN** the TUI renders it as permanent transcript content
- **AND** a plan snapshot is represented by a compact change record with available progress and current-step information
- **AND** the corresponding full plan remains inspectable subject to explicit retention limits

#### Scenario: A historical plan is inspected after a newer update
- **WHEN** the user opens the detail of an earlier plan record
- **THEN** Alan shows that record's full snapshot or its explicit unavailability
- **AND** it does not substitute the newest plan or infer missing steps
- **AND** closing detail restores the same draft and transcript position

#### Scenario: A plan observation repeats without a change
- **WHEN** an already represented plan snapshot is observed again
- **THEN** the renderer does not append a duplicate plan change record
- **AND** distinct later snapshots remain separately represented
