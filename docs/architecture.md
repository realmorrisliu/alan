# Alan Architecture

Alan separates file-system substrate, Process lifecycle, agent computation,
and rendering into explicit owners.

## Layering

```text
Alan for macOS / Rust TUI / future apps
                  |
       mounted files and control writes
                  v
AgentFS + File-Server Services + Alan Shell
                  |
              aP operations
                  v
Alan Kernel: namespace, mounts, descriptors, Process table, /proc, /srv
```

The Agent Execution Engine backs Agent Runtime Service work above Alan Kernel.
It owns the AI Turing-machine transition loop, not OS identity or namespace
semantics.

## Ownership map

| Concern | Owner |
| --- | --- |
| PID, parent, credentials, descriptors, lifecycle, exit | Process and `/proc` |
| tape and transition-local state | Agent Machine |
| agent IO, requests, actions, plans, machine files | AgentFS |
| durable execution evidence | rollout and checkpoint files |
| cross-Process continuity | Memory Stores and handoff files |
| service discovery | `/srv` handles |
| provider generation | LLM Connections and provider adapters |
| Tool effects | spawned Tool Processes and file writes |
| terminal presentation | renderer hosts |
| app domain truth | app-owned domain core and file-server adapter |

## Current startup

The `alan` binary currently hosts direct CLI commands and the linked Rust TUI.
The TUI receives a mounted namespace plus an Agent Process path, hydrates from
AgentFS snapshots, tails offset-readable files, and writes input or control
commands back through the mounted tree.

The complete Service Manager boot sequence is still under construction. Target
crate ownership is recorded in [ADR-0025](adr/0025-target-crate-architecture.md).

## Agent definitions

An Agent Definition is one explicitly supplied file tree containing optional
`agent.toml`, `persona/`, `skills/`, and `policy.yaml`. A Process receives it by
descriptor. Alan does not derive an overlay chain from Host cwd, home, or
directory names, and definition layout does not imply Process ancestry.

## Persistence

Durable service state is channel-isolated in the Host-selected System Store:

```text
Alan/System Store/<channel>/services/
├── agent-runtime/{rollouts,checkpoints,cache,tmp,metadata}/
├── connections/connections.toml
├── memory/stores/
└── packages/
```

A rollout uses its own record id and records the producing Process path.
Recovery creates a new Process and a new rollout from an explicitly selected
source record. Memory Stores are explicit file trees and use Process
provenance. Live Process tables, PIDs, descriptors, namespaces, and tasks are
ephemeral.

## Boundary rules

- Alan Kernel depends only on aP among Alan crates.
- provider, sandbox, terminal, app, and platform details stay above Kernel.
- lifecycle is never inferred from an app snapshot.
- derived UI state is not a second source of domain truth.
- live child state is read from `/proc`; delegation metadata is bounded and
  Process-local.
- future Alan for macOS attachment requires a separate accepted design; see
  [ADR-0029](adr/0029-remove-daemon-era-surfaces-before-replacement-design.md).
