## Context

Alan Agent is not Root Agent Process, Agent Runtime Service, Service Manager, or
the required entrypoint for agent work. It is a built-in optional workspace over
Agent Processes. Alan Shell must remain able to operate the system directly
through `/agent`, `/proc`, `/lib/skill`, `/man`, `/mnt/mem`, `/mnt/policy`,
requests, actions, and spawn/open/watch syscalls.

## Goals / Non-Goals

**Goals:**

- Define Alan Agent as built-in but optional.
- Preserve current session behavior during migration.
- Project current session, event, tool, yield, child-agent, memory, rollout, and
  plan data into Agent Process workspace views.
- Let Alan Shell remain the first and primary compatibility host.
- Keep future Alan for macOS host integration aligned with the same file/process
  contract.

**Non-Goals:**

- Implement Root Agent Process resident behavior.
- Make Alan Agent required for app AI or user agent work.
- Replace current compatibility session paths in the first slice.
- Build a complete macOS host migration.

## Decisions

### 1. Alan Agent is optional workspace UI

Alan Agent owns a richer workspace experience: process browsing, steering,
inspection, memory review, evidence browsing, request/action review, and
promotion of cross-app work into a focused workspace. It does not own execution.

### 2. Agent Processes are the semantic execution unit

Current sessions remain compatibility authority during migration, but the target
is reading the agent file surfaces: status, IO, requests, actions, children,
context, and machine state (results are IO output plus per-action
`actions/<id>/result`, not a top-level `result` file).

### 3. Alan Shell remains primary

Alan Shell is the primary Alan OS interaction surface. It can list `/agent`,
inspect `/agent/root/status`, tail events, spawn agent executables, answer
requests, and operate Tools and Skills without opening Alan Agent.

### 4. Root Agent work appears as workspace items

Root Agent Process should not appear as a global chat session. When it raises
work, Alan Agent can show root-agent-raised suggestions or tasks backed by
AgentFS files and descriptors.

## Risks / Trade-offs

- [Risk] Compatibility session UX and Agent Process projection diverge. -> Run
  projection in parallel and add parity fixtures before replacing reducers.
- [Risk] Alan Agent becomes required for app AI. -> Keep Alan Shell and app
  spawn/open/watch paths canonical.
- [Risk] Workspace tries to ship Root Agent runtime too early. -> Represent
  root-agent-raised work as projections first; resident Root Agent Process
  behavior is separate work.
