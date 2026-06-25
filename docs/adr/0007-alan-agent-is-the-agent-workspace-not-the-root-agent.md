# Alan Agent Is The Agent Workspace, Not The Root Agent

Alan Agent should be the built-in user-visible Agent Workspace for inspecting,
steering, and organizing agent work, while the Root Agent remains
the always-available system intelligence layer. Other Alan Apps start agent work
the same way — by spawning Agent Executables and reading/writing AgentFS files —
without routing through the Alan Agent UI (Agent Capability / Agent Run / Context
Grant APIs are retired; ADR-0024). Alan Agent can surface root-agent-raised
tasks, memory, evidence, and cross-app continuity.
