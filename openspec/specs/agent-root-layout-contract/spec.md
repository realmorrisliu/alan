# agent-root-layout-contract Specification

## Purpose
Define the implementation boundary that keeps production code using
runtime-owned typed agent-root layout APIs instead of duplicating canonical path
construction. This capability depends on `agent-root-layout` for external path
semantics and owns code-level centralization, non-Rust mirror constraints, and
guardrails.
## Requirements
### Requirement: Runtime-owned agent-root layout
alan SHALL expose a runtime-owned typed API for canonical agent-root layout
construction. Production Rust code outside the layout owner SHALL use that API for
default roots, named roots, and standard agent-root asset paths.

#### Scenario: Default root paths are requested semantically
- **WHEN** a caller needs the global or workspace default agent root
- **THEN** the caller can request the default root through the runtime layout API
- **AND** the caller does not need to join literal `agents/default` path segments

#### Scenario: Standard asset paths are requested semantically
- **WHEN** a caller needs `agent.toml`, `persona/`, `skills/`, or `policy.yaml` under an agent root
- **THEN** the caller can request the asset path through the runtime layout API
- **AND** the returned path uses the canonical agent-root layout

### Requirement: Raw layout-string guardrail
alan SHALL provide a mechanical guardrail that detects new raw canonical agent-root
layout strings in Rust production code outside approved layout-owner locations.

#### Scenario: Production code adds a raw default-root string
- **WHEN** production Rust code outside the runtime layout owner introduces a raw string such as `.alan/agents/default`
- **THEN** the guardrail reports the occurrence
- **AND** the fix is to use the runtime layout contract or add an explicit allowlist entry with justification

#### Scenario: Tests and documentation use literal paths
- **WHEN** tests, documentation, or OpenSpec artifacts use literal canonical paths to describe the external contract
- **THEN** the guardrail allows those paths through an explicit scope or allowlist
- **AND** the allowed usage does not become a production-code layout owner

### Requirement: Agent-name semantics are shared by direct launch callers

Alan SHALL centralize agent-name normalization, validation, and `default` reservation semantics for CLI commands and Agent Process launch paths.

#### Scenario: Explicit default is normalized

- **WHEN** a CLI or Agent Process launch caller supplies agent name `default`
- **THEN** the runtime layout API selects the default root chain
- **AND** it does not construct a named overlay for `default`

#### Scenario: Named agent is validated

- **WHEN** a direct caller supplies a named agent value
- **THEN** the runtime layout owner validates it as a safe single path component
- **AND** the caller does not duplicate traversal or normalization rules

### Requirement: Direct readers and writers share the runtime layout owner

Alan SHALL use the runtime-owned layout contract for agent-root reads and for direct CLI or authoring writes. Production consumers MUST NOT construct canonical agent-root paths independently.

#### Scenario: Setup writes a loadable default config

- **WHEN** `alan init`, connection pinning, or another direct authoring flow writes a default `agent.toml`
- **THEN** it writes the path returned by the runtime layout owner
- **AND** Agent Process definition resolution reads the same path without a transport-specific mapping

#### Scenario: Renderer receives an explicit mounted root

- **WHEN** a renderer or host needs to inspect an Agent Process definition or file surface
- **THEN** it receives a typed path or mounted root from the owning boundary
- **AND** it does not recompute canonical paths from a duplicated client mirror
