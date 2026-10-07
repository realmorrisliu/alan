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
