# Target Contract Readiness

`define-agent-capability-os-model` is ready to be treated as a target
contract for follow-up implementation changes.

## Decision

Ready, with implementation split into three follow-up changes:

1. `add-agent-capability-kernel-types`
2. `add-agent-capability-service-adapter`
3. `migrate-alan-agent-to-agent-workspace`

## Rationale

- The OS model now separates System Agent Supervisor, bounded Agent Runs,
  Agent Capability Service, Context Grants, Result Contracts, Command
  Governance, memory ownership, and Alan Agent Workspace.
- Existing Alan Agent implementation has a migration map that preserves current
  runtime, governance, sandbox, memory, rollout, child-run, daemon, and TUI work
  without copying implementation details into Alan Kernel.
- `define-programmable-environment-product` now treats Agent Capability as a
  standard Alan OS ability.
- `introduce-alan-kernel-runtime` now keeps Agent Capability Service execution out
  of `alan-kernel`.
- The first descriptor taxonomy is intentionally narrow enough to implement:
  `explain`, `summarize`, `plan`, `propose_commands`, and `delegate`.

## Remaining Design Pressure

- Memory ownership details still need their own implementation work before
  `agent.remember` becomes a V1 descriptor.
- System Agent Supervisor should not ship as a resident root session before the
  Agent Capability Service compatibility adapter exists.
- The first visible supervisor-raised task surface should be Alan Agent, rendered
  first through the Alan TUI compatibility host; Alan for macOS can follow the
  same host contract later.

