## Context

Alan Agent is not the System Agent Supervisor and not the whole Alan OS. It is
the user-visible workspace for agent work. That means it should show and steer
bounded Agent Runs, long-lived compatibility sessions, memory, evidence,
supervisor-raised tasks, and cross-app agent work without forcing every app to
route through the Alan Agent UI.

## Goals / Non-Goals

**Goals:**

- Define the Alan Agent app module as the Agent Workspace.
- Preserve current session behavior during migration.
- Project current session, event, tool, yield, child-run, memory, rollout, and
  plan data into semantic objects, buffers, views, tasks, forms, evidence, and
  audit.
- Let Alan TUI render the first Agent Workspace semantic projections.
- Keep future Alan for macOS host integration aligned with the same contract.

**Non-Goals:**

- Implement System Agent Supervisor resident behavior.
- Force domain apps to open Alan Agent UI for ordinary Agent Capability calls.
- Replace current daemon session APIs in the first slice.
- Build a complete macOS host migration.

## Decisions

### 1. Alan Agent owns the workspace experience

Alan Agent owns conversation organization, session/workspace navigation,
steering, inspection, memory review, evidence browsing, and promotion of
cross-app work into a full workspace.

### 2. Agent Runs are the semantic execution unit

Current sessions remain compatibility authority, but Agent Workspace projection
should show bounded Agent Runs and their task/evidence relationships.

### 3. TUI moves first

Alan TUI is already a thin daemon-backed client and is the least risky first
host for Agent Workspace projections. Alan for macOS should later consume the
same snapshots rather than invent a different app model.

### 4. Supervisor tasks appear as workspace items

The System Agent Supervisor should not appear as a global chat session. When it
raises work, Alan Agent can show supervisor-raised tasks that the user can
inspect, dismiss, delegate, or promote into an Agent Workspace flow.

## Risks / Trade-offs

- [Risk] Compatibility session UX and Agent Run projection diverge. -> Run the
  semantic path in parallel and add parity fixtures before replacing reducers.
- [Risk] Alan Agent becomes required for app AI. -> Keep app Agent Capability
  calls direct; Alan Agent is for inspection and steering.
- [Risk] Workspace tries to ship supervisor runtime too early. -> Represent
  supervisor-raised tasks as projections first; resident supervisor behavior is
  separate work.

