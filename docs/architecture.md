# Alan Architecture

Alan separates file-system substrate, Process lifecycle, agent computation,
and rendering into explicit owners.

## Layering

```text
Terminal host (prefer Herdr) → Alan CLI / terminal renderer / future apps
                  |
       mounted files and control writes
                  v
AgentFS + File-Server Services + Alan Shell
                  |
              aP operations
                  v
alan9 Kernel: namespace, mounts, descriptors, Process table, /proc, /srv
```

The Agent Execution Engine backs Agent Runtime Service work above alan9 Kernel.
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
| model operations | LLM Connections and provider adapters; finite-choice evaluation is delivered |
| Tool effects | spawned Tool Processes and file writes |
| terminal presentation | renderer hosts |
| app domain truth | app-owned domain core and file-server adapter |

## Current startup

Each bare `alan` invocation with `ALAN_INSTANCE_RUNTIME_DIR` unset or set to a
directory not in use by another invocation starts and owns one foreground
alan9 instance and its Root Agent Process. With terminal stdin and stdout it
runs the file-backed Agent renderer against that instance's `/agent/root`;
with redirected stdin it submits one task to the same instance and writes the
answer to stdout and diagnostics to stderr without starting the interactive
renderer. Herdr sessions using separate runtime directories have separate
Roots and endpoints while using the same product stores. Reusing one explicit
runtime directory allows only one owner; another invocation fails to acquire
it rather than sharing its Root. Actual Alan process exit shuts down its
instance; a terminal-host view detach is separate from process exit. New
invocations start fresh by default; `alan --resume` explicitly selects durable
recovery with current-authority validation and paused pending work. Interactive
`!` requests an exact command through the governed native Tool path; `:` selects
Agent reasoning. PR #1032 records recovery, view-detach, revocation and no-repeat
acceptance. Broader platform and workflow evidence remains separately qualified.
See [ADR-0056](adr/0056-bare-alan-attaches-to-root-agent.md).

Alan for macOS is retired; its App, helper and shell-core/FFI source has been
removed. Rust Host platform adapters remain. Herdr supplies terminal topology,
not Alan execution authority.
See [ADR-0054](adr/0054-retire-desktop-client-prefer-terminal-hosts.md).

The current engine is generation-driven. The accepted mixed Machine direction
is in [ADR-0055](adr/0055-agent-machine-composes-typed-capabilities.md); finite-choice
evaluation, no-effect shadow advice and one bounded read-only mixed program have
shipped. General mixed transitions and complete state/projection alignment remain
unfinished; automatic input routing is disabled. Existing
rollout/checkpoint recovery is not equivalent to the namespace text Tape alone.

Package Service currently distributes Skills. Embedded boot units and Tool
registry entries do not yet come from q executable installation. Target crate
ownership is recorded in [ADR-0025](adr/0025-target-crate-architecture.md).

## Agent definitions

An Agent Definition is one explicitly supplied file tree containing optional
`agent.toml`, `persona/`, `skills/`, and `policy.yaml`. A Process receives it by
descriptor. Alan does not derive an overlay chain from Host cwd, home, or
directory names, and definition layout does not imply Process ancestry.

## Persistence

Durable service state lives in the Host-selected product System Store:

```text
Alan/System Store/services/
├── agent-runtime/{rollouts,checkpoints,cache,tmp,metadata}/
├── connections/connections.toml
├── memory/stores/
└── packages/
```

A rollout uses its own record id and records the producing Process path. The
explicit recovery flow creates invocation-local execution from selected durable
evidence, validating current authority and retaining unresolved work paused. Memory Stores are explicit file
trees and use Process provenance. Live Process tables, PIDs, descriptors,
namespaces, and tasks are ephemeral.

## Boundary rules

- alan9 Kernel depends only on aP among Alan crates.
- provider, sandbox, terminal, app, and platform details stay above Kernel.
- lifecycle is never inferred from an app snapshot.
- derived UI state is not a second source of domain truth.
- live child state is read from `/proc`; delegation metadata is bounded and
  Process-local.
- terminal-host integration cannot grant mounts, credentials or action approval;
  the foreground CLI composition owns instance shutdown, while a view detach is
  distinct from actual Alan process exit.

## Accepted next delivery

[ADR-0060](adr/0060-explicit-delegation-separates-approval-and-isolation.md) separates
project delegation, approval posture and actual execution isolation. Host Mount
Service remains file-grant authority; Agent Runtime owns action policy; Guardian
judges eligible actions without granting rights. System 1 is a work mode, not a
new Process or privileged service. Current Guardian uses the main Connection and
fixed default policy; dedicated reviewer configuration is a tracked delivery gap.

Project continuity extends existing evidence and Memory Stores. It does not make
Root permanent or introduce a global Project executor. Fully trusted native
Host-account execution is a separately accepted future option with no same-account
tamper-resistance claim. No proposed CLI flags are available merely because this
architecture note exists. See the [ordered plan](../openspec/changes/qualify-personal-daily-development/roadmap.md).
