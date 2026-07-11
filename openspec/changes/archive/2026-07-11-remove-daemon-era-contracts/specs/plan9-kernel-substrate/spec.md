## MODIFIED Requirements

### Requirement: The kernel crate is dependency-isolated

The `alan-kernel` crate SHALL depend only on `alan-ap`, the aP protocol contract. Agent execution, LLM providers, tape, memory, policy, sandboxing, renderer concerns, service implementations, and byte transports SHALL live in user-space file-server crates and adapters above Alan Kernel.

#### Scenario: Kernel crate dependencies are audited

- **WHEN** `alan-kernel` dependencies are reviewed
- **THEN** they include `alan-ap` and exclude Agent Execution Engine, AgentFS service, provider, Memory Store, sandbox backend, renderer, and transport implementation crates
- **AND** agents, providers, memory, and Tools appear to Alan Kernel only through Processes, descriptors, namespaces, mounts, and file-server trees
