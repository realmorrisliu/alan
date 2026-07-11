## REMOVED Requirements

### Requirement: Request control intent is separate from resolved controls
**Reason**: Intent layers include Session-level overrides.
**Migration**: Separate Agent Process configuration intent from per-turn intent and resolved provider controls.

### Requirement: Runtime-owned request control resolution
**Reason**: Precedence and examples are Session-centered and name daemon recomputation.
**Migration**: Resolve Process defaults once and apply per-turn overrides in Agent Machine execution.

### Requirement: Daemon and clients mirror resolver metadata
**Reason**: Daemon Session, fork, and client DTO metadata are removed.
**Migration**: Project effective controls through Agent Machine and provider-owned files where needed.

### Requirement: Reasoning effort observability
**Reason**: Observability is defined through runtime/Session metadata and daemon listing.
**Migration**: Expose effective controls through Agent Machine state and rollout/checkpoint evidence.

### Requirement: Documentation and migration
**Reason**: Migration guidance includes Session and daemon compatibility surfaces.
**Migration**: Document Process and turn resolution only.

### Requirement: Request control tests guard layer boundaries
**Reason**: The current matrix requires Session overrides and daemon metadata mirroring.
**Migration**: Test Process config, turn override, model default, provider mapping, and file projection owners.

## ADDED Requirements

### Requirement: Request control intent separates Process and turn ownership
Alan SHALL represent Agent Process request-control intent separately from per-turn intent and from
the normalized controls passed to a provider. Process intent SHALL be resolved from the AgentRoot,
workspace overlays, connection/model catalog, and spawn inputs; turn intent MAY override only the
current transition.

#### Scenario: A turn overrides Process reasoning effort
- **WHEN** an Agent Process resolves medium reasoning effort and one turn explicitly requests low
- **THEN** that generation uses low
- **AND** later turns retain the Process-level medium intent

### Requirement: Effective request controls are file and rollout observable
Agent Runtime Service SHALL project effective Process and current-turn request controls through
Agent Machine state and rollout/checkpoint evidence.

#### Scenario: Renderer or auditor inspects effective controls
- **WHEN** effective reasoning controls are needed for inspection
- **THEN** the client reads the owning Agent Machine or durable evidence surface
- **AND** the projected values come from the canonical runtime resolver

### Requirement: Request control tests guard durable owners
Tests SHALL cover Agent Process intent, per-turn override, AgentRoot configuration, model catalog
default, provider projection, and Agent Machine/rollout observability. They SHALL fail if a renderer,
transport adapter, or provider adapter independently recomputes resolver-owned defaults.

#### Scenario: Resolver ownership drifts
- **WHEN** request-control resolution is duplicated outside the canonical runtime resolver
- **THEN** focused boundary tests fail
