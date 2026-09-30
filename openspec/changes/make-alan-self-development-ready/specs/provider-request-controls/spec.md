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
