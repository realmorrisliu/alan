## Context

Alan for macOS currently owns terminal presentation/runtime and shell-core
state, while the Agent runtime product path is the linked Rust TUI. The system
Host and Service Manager establish a stable aP attachment seam that macOS can
consume without embedding Alan OS.

## Goals / Non-Goals

**Goals:**

- Make macOS a renderer/native-capability host for the matching channel Host.
- Add Shell and Agent views without copying Process or Agent Machine truth.
- Preserve live attachment across macOS app/view restart.
- Keep host presentation and Alan OS command planes distinct.

**Non-Goals:**

- App-owned Alan OS boot, terminal runtime redesign, package management,
  remote attachment, or Process restoration after Alan OS Host restart.

## Decisions

### 1. App attaches; it never boots Alan OS internally

Stable/dev resolve only their channel endpoint. App startup requests platform
Host start if absent, connects over aP, validates readiness/channel, and asks
Local Entry Service for one app-level Shell Process. Windows share it while
Agent views attach independently.

### 2. Agent ContentInstance stores an Agent Attachment

The persisted payload contains Process Reference (boot ID + PID), caller-held
stream offsets, and presentation. AgentFS paths derive from the verified PID.
Tape, requests, Tool state, provider state, status authority, and socket objects
remain outside shell persistence.

### 3. Reattachment never recreates execution

On app/view recreation, verify boot ID and `/proc/<pid>`, reopen AgentFS files,
and continue from saved offsets with overlap dedupe. Exited Process may show its
terminal evidence; missing/mismatched Process becomes unavailable. Creating a
new Process from durable evidence is an explicit user action.

### 4. Closing a view detaches

Pane/Tab/window/app close releases renderer fids only. Stop is a distinct
write to `/proc/<pid>/ctl`, with any lineage behavior owned by Alan OS policy.

### 5. Native adapters answer service requests

Directory picker/security scope and Keychain/browser login remain Swift/native
adapters for Host Mount and Connection services. They return grant or opaque
credential results, never become profile/mount authority.

## Risks / Trade-offs

- [PID reuse attaches wrong Process] → Require matching boot ID and verify
  `/proc` qid/reference before reading AgentFS.
- [Stream gaps or duplicate UI] → Persist caller offsets, resume by byte offset,
  dedupe overlap, and surface retention gaps.
- [Shell manifest becomes Agent state cache] → Schema guards permit only
  reference, offsets, and presentation.
- [App close unexpectedly stops work] → Add lifecycle tests proving Host and
  Agent Processes remain alive.

## Migration Plan

1. Add Swift aP client and channel Host discovery/start adapter.
2. Attach one Shell Process and expose basic namespace Shell rendering.
3. Add Agent ContentInstance model, persistence, and file-backed renderer.
4. Add reattachment/detach/explicit stop behavior.
5. Wire Host Mount and Connection native request adapters.
6. Remove any app-owned Agent runtime boot code and complete fresh-app E2E.

## Open Questions

None. Terminal ContentInstance ownership remains unchanged.
