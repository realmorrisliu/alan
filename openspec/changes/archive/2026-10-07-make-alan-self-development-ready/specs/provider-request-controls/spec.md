## MODIFIED Requirements

### Requirement: Reasoning-capable model metadata
Alan SHALL declare model-level supported and default reasoning efforts in the
model catalog. The Connection-owned catalog projection used by model selection
SHALL expose those values from the canonical model owner, without a second
renderer-owned catalog. Unavailable metadata SHALL remain explicitly unavailable.

#### Scenario: Model catalog entry declares efforts
- **WHEN** a bundled or overlay model entry supports reasoning
- **THEN** the entry can declare `supported_reasoning_efforts` and `default_reasoning_effort`

#### Scenario: Default effort must be supported
- **WHEN** a model entry declares `default_reasoning_effort`
- **THEN** Alan validates that the default appears in `supported_reasoning_efforts`

#### Scenario: Existing supports_reasoning compatibility
- **WHEN** an existing catalog entry only declares `supports_reasoning = true`
- **THEN** Alan derives a conservative supported/default effort set or requires the entry to be migrated before validation passes

#### Scenario: Owner-visible model metadata
- **WHEN** the model catalog exposes model metadata to an authorized consumer
- **THEN** it includes supported reasoning efforts and the default reasoning effort for each listed model
- **AND** the Connection projection preserves those owner-provided values

#### Scenario: Default managed configuration uses the canonical bundled catalog
- **WHEN** a published managed connection uses a model recognized by the canonical bundled catalog without an explicit catalog override
- **THEN** its model list and reasoning metadata come from the same resolved catalog used to validate model selection
- **AND** absence of an optional override does not make that known catalog unavailable
- **AND** a provider without canonical model information remains explicitly unavailable

#### Scenario: Frequent model observation uses published Connection state
- **WHEN** the runtime observes model state without an explicit Connection operation
- **THEN** its catalog projection reads the Connection Service's already-published authority and canonical metadata
- **AND** observation does not reload persisted connection metadata or construct a new callable
- **AND** Connection-owned publication changes remain visible on the next observation
- **AND** external metadata changes are refreshed at existing explicit Connection read, capture, selection and recovery boundaries, without claiming an instantaneous cross-process disk watch
- **AND** observation never bypasses refreshed validation of an explicit capture or selection

### Requirement: Effective request controls are file and rollout observable
Agent Runtime Service SHALL project effective Process and current-turn request
controls through Agent Machine state and rollout/checkpoint evidence. Inspection
SHALL distinguish the confirmed binding for subsequently admitted input from the
captured binding and resolved controls of already-admitted work. Unknown values
SHALL be explicit; no secret material SHALL enter these projections.

#### Scenario: Renderer or auditor inspects effective controls
- **WHEN** effective reasoning controls are needed for inspection
- **THEN** the client reads the owning Agent Machine or durable evidence surface
- **AND** the projected values come from the canonical runtime resolver

#### Scenario: Model selection occurs while older work remains admitted
- **WHEN** a client inspects status after a confirmed model selection
- **THEN** it can distinguish the confirmed next-input binding from older admitted work's captured binding
- **AND** the renderer does not relabel that older work with the newly selected model

#### Scenario: Model selection receipt belongs to one Process input
- **WHEN** the renderer requests a model selection
- **THEN** it correlates completion with the pinned Process and the exact selection input ID
- **AND** successful control-file transport or an unrelated completion does not confirm selection
- **AND** a response from an earlier Root cannot settle the current request

#### Scenario: Model observation and selection completion arrive separately
- **WHEN** a model projection update and a selection completion arrive in either order
- **THEN** the renderer distinguishes the observed selected-next binding from the request's pending or completed disposition
- **AND** it does not fabricate model controls when the projection cannot be read
- **AND** losing the owning Process or completion stream leaves the request uncertain without automatic retry

#### Scenario: A completed model selection is retried after a later selection
- **WHEN** a client repeats a model selection with the same input ID after a different model has been confirmed
- **THEN** the original terminal outcome is returned without installing or persisting that selection again
- **AND** subsequent ordinary input captures the later confirmed binding
- **AND** restoring durable selection outcomes preserves the same identity fence without replaying model selection

#### Scenario: A failed model selection is repeated
- **WHEN** a selection has a retained terminal failure and the same input ID is delivered again
- **THEN** it returns the same failed outcome without trying a new selection
- **AND** a durable failure outcome does not replace the last successfully confirmed binding
