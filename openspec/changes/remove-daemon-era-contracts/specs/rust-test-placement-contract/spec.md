## REMOVED Requirements

### Requirement: Rust test placement vocabulary is stable

**Reason**: The Live test definition names the daemon as a standard live environment.

**Migration**: Define Live tests around real providers, Process hosts, AgentFS mounts, and other surviving environments.

### Requirement: Extracted white-box tests preserve private access without bloating implementation files

**Reason**: The extraction guidance preserves daemon internals as an example private owner.

**Migration**: Apply the same placement rule to private Agent Engine, Process, file-server, policy, provider, and renderer internals.

### Requirement: Integration tests cover public crate and process boundaries

**Reason**: HTTP route and WebSocket contracts are listed as canonical integration boundaries.

**Migration**: Integration tests cover public crates, CLI processes, AgentFS files, persistence, and live runtime boundaries.

## ADDED Requirements

### Requirement: Rust test placement vocabulary uses current runtime owners

Alan SHALL use stable Rust test placement vocabulary across OpenSpec, AGENTS.md, review guidance, and crate documentation.

- **Inline unit test**: a small `#[cfg(test)] mod tests` block in the implementation file.
- **Extracted white-box test file**: a test-only child module that exercises private details without widening production visibility.
- **Integration test**: a crate-level test under `crates/<crate>/tests/` that exercises an external crate, CLI process, or durable file boundary.
- **Live test**: an opt-in integration test against a real provider, Process host, mounted namespace, or other live surviving environment.
- **Test support helper**: fixture or assertion code compiled only for tests.

#### Scenario: Test placement is classified

- **WHEN** current docs, review comments, or OpenSpec changes classify a Rust test
- **THEN** they use these terms with the stated meanings
- **AND** they do not preserve a removed transport as a standard test category

### Requirement: Extracted white-box tests preserve private access for current owners

Alan SHALL place large private-access suites in extracted white-box files adjacent to the implementation owner rather than widening production visibility or bloating implementation files.

#### Scenario: Private Agent Engine suite grows

- **WHEN** an Agent Engine, Process, AgentFS, policy, provider, Tool, or renderer suite needs private access and substantial fixtures or async orchestration
- **THEN** it moves to an adjacent test-only module tree
- **AND** its helpers are not imported by production code

#### Scenario: Flat module needs extracted tests

- **WHEN** `foo.rs` has a large private-access suite
- **THEN** it loads an adjacent `foo_tests.rs` under `#[cfg(test)]` or converts to a directory-backed module layout

### Requirement: Integration tests cover public crate, CLI, Process, and file boundaries

Alan SHALL use `crates/<crate>/tests/` for black-box behavior validated through public crate APIs, CLI processes, Process and AgentFS boundaries, persistence records, or live provider/runtime harnesses.

#### Scenario: AgentFS contract is tested

- **WHEN** a test validates public Process launch, AgentFS IO, request, action, machine, offset, or control behavior
- **THEN** it exercises the public crate or process/file boundary from a crate-level integration test
- **AND** it does not require private production visibility

#### Scenario: Live provider test is added

- **WHEN** a Rust test talks to a real provider or live Process environment
- **THEN** it is an explicitly opt-in integration test, normally ignored by default
- **AND** its required environment is documented
