# Spec Delta

## ADDED Requirements

### Requirement: Task budgets bound autonomous continuation
Every autonomous task SHALL have finite configured elapsed-time, Tool-call and model-call budgets, including review, fallback and retry overhead. Exhaustion SHALL pause further admission and preserve recoverable evidence; resume MUST NOT reset consumption without explicit user extension. Unavailable monetary usage SHALL NOT be reported as known cost.

#### Scenario: Budget exhausted
- **WHEN** a task reaches any configured execution budget
- **THEN** further dependent work pauses with a readable reason and retained state

#### Scenario: Task resumes after restart
- **WHEN** a user selects a paused task for recovery
- **THEN** its consumed budget remains accounted and no completed effect is replayed

#### Scenario: Shadow advice consumes the remaining allowance
- **WHEN** no-effect input advice exhausts the task budget before ordinary generation
- **THEN** the original input and settled evaluation evidence remain available at the budget pause, no further paid model or Tool admission occurs before explicit extension, and uncertain evidence or user cancellation is not treated as clean exhaustion

#### Scenario: Cancellation cleanup exceeds the time allowance
- **WHEN** already-admitted work settles after the elapsed-time limit
- **THEN** an explicit extension request covers the recorded overrun and a fresh finite tranche, includes other exhausted dimensions, and never refunds consumed time or calls

#### Scenario: The original model binding cannot be restored
- **WHEN** a budget pause carries a selectable Connection binding whose captured revision is unavailable or revoked
- **THEN** recovery fails without silently substituting the current default or selected-next model

#### Scenario: A bounded mixed program waits for additional budget
- **WHEN** the existing source-owner Machine program exhausts its task allowance during source capture, model selection or validation
- **THEN** its budget checkpoint preserves the exact unfinished read and program progress, explicit extension resumes the program without replaying completed effects, repeating settled model calls or substituting ordinary generation; resumed final validation rechecks current source authority and bytes within an explicitly sufficient finite allowance, and changed evaluator authority or uncertain settlement never permits an implicit retry

#### Scenario: Descendant work shares the task allowance
- **WHEN** an Agent Process creates descendants during an admitted task
- **THEN** Agent Runtime Service captures the original ancestor ledgers, model and Tool admission charges each applicable ancestor once, concurrency cannot overspend them, and a new parent task or child-local extension cannot replace or enlarge the captured allowance

#### Scenario: A parent stops while descendants are cleaning up
- **WHEN** a parent pauses, cancels or finishes while admitted descendant operations remain unsettled
- **THEN** further descendant admission stops, in-flight operations are cancelled and awaited, cleanup time remains charged, and a crash during that unsettled interval cannot recover a free paused allowance or a fresh task identity

#### Scenario: Foreground completion waits for descendant cleanup and finalization
- **WHEN** a foreground answer, fallback or cancellation finishes while admitted descendants are still cleaning up
- **THEN** its terminal completion event is withheld until cleanup and owning submission finalization succeed, terminal source-owner projections do not announce idle before that cleanup, final evidence and IO time remain charged, and uncertain finalization does not publish a successful completion

#### Scenario: An Agent Process is forcibly ended
- **WHEN** Kernel control or the host ends an Agent Process while its captured descendant work remains in flight
- **THEN** the Process exit closes admission for its captured ledgers and descendants, admitted descendant operations cancel and settle through existing cleanup before reporting failure, consumed budget is retained without offering child-local extension for a dead ancestor, and a replacement Process does not revive the exited Process's captured work

#### Scenario: Runtime future is dropped during an allocated operation
- **WHEN** forced Process termination drops a runtime with an allocated model or Tool operation
- **THEN** adapter-owned guards retire that operation through bounded existing controls, runtime exit waits for tracked retirement before closing its writer, and timeout, failed acknowledgement or cleanup panic remains an unconfirmed outcome rather than successful completion or permission to replay the effect

#### Scenario: Descendant evidence is recovered without its parent authority
- **WHEN** recovery cannot bind the descendant's recorded ancestor task identities to the original live admission bounds
- **THEN** startup fails before model or Tool admission instead of turning the descendant into an independent task with fresh limits

### Requirement: Pending decisions preserve dependency ordering
A pending permission or clarification SHALL keep its original correlated operation. Other work SHALL proceed only if it is independently authorized and does not consume that operation’s result or rely on its unresolved authority. Unknown dependence SHALL preserve ordering, and repeated observations SHALL NOT create duplicate prompts or effects.

#### Scenario: Independent read exists
- **WHEN** a pending publication decision does not affect an already-admitted read-only review
- **THEN** the runtime may complete the independent review while retaining the publication wait

#### Scenario: Dependent work exists
- **WHEN** a later operation requires the pending operation’s output
- **THEN** it does not execute before that decision and result settle
