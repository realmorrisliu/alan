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
- **AND** request responses correlate by request identity across repeated deliveries and recovery
- **AND** this evidence does not grant response acceptance, settle ordinary input or dispatch effects
- **AND** unknown/stale response IDs and Host Mount Service controls do not reserve bypass identity
