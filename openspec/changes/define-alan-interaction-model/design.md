## Context

Alan OS converges on a Plan 9 model: a volatile kernel, one system-level
instance per channel (ADR-0044), local hosts attaching over aP Unix sockets
(ADR-0045), renderers persisting only Process References and offsets
(ADR-0046/0047), and Host Mount Service as the sole grant authority
(ADR-0050). These decisions already make every user-visible surface a live
view of files. What is missing is the product contract above them: who the
user is, what objects they manipulate, and which OS concepts may reach the
default UI.

The audience decision for this contract is: **advanced personal users** —
people who want leverage from agents and will inspect, script, and customize
when given the layer, but who do not think in mounts and namespaces and should
never have to. The interaction-mode decision is: conversation is one mode
among three; the primary posture is **agents as background servants whose
results the user reviews**, extended by **event-driven behavior** where agents
act on triggers and report outcomes.

## Goals

- One interaction model that every renderer host (macOS, TUI, future Alan
  Apps) implements, so users learn it once.
- The file-native architecture remains fully reachable and scriptable — the
  model adds layers over the truth, never a parallel truth.
- OS vocabulary is quarantined: default UI copy passes a "user object" test.
- Mounting, approvals, and stopping work reuse the grant and `/proc/<pid>/ctl`
  mechanisms the system already provides; UX introduces no new authority.

## Non-Goals

- Runtime event/trigger machinery (schedulers, watchers, rule storage): this
  change defines only how event-driven behavior appears to the user.
- Visual design tokens and material treatment: owned by
  `macos-shell-ui-ux-conformance`.
- Onboarding flows, empty states, and first-run copy: a later change.
- TUI-specific keybindings and macOS-specific layout details.

## Decisions

### D1: The file system is the API, not the UI

The namespace is how agents, apps, and hosts share truth — like iOS's file
system, it is infrastructure, not a user concept. Unlike iOS, the file layer
is never locked: it is the inspect-and-program layer (D2, Files). The
differentiating bet versus black-box agent products is that every UI element
is backed by a real file, so peeling back is always possible and always safe.

### D2: Three disclosure layers over one truth

- **Intent layer**: the user states what they want; an agent works. Objects:
  intent text, agents, results.
- **Work layer**: agent file surfaces rendered as native affordances —
  `machine/tape` as conversation, `machine/ui/plan` as a plan card, pending
  actions as approval sheets, `/proc/<pid>/ctl` as a Stop button,
  rollouts as evidence views. User gestures become file writes and `ctl`
  commands.
- **Files layer**: the raw namespace as an explicit mode ("view as files",
  an inspector, the shell tab). This is where mounts, paths, and `/proc`
  vocabulary legitimately live, and where programmability starts.

Layers are views, not tiers of capability: anything doable in the Work layer
is a file operation visible in the Files layer, and vice versa. This follows
ADR-0046 — renderers hold references and offsets, never copied state — so
there is nothing to synchronize.

### D3: Three interaction modes, conversation is not the entry assumption

- **Conversation**: direct dialogue with an agent. One mode, not the default
  posture.
- **Background servant**: the user dispatches work; agents run detached
  (closing a view detaches per ADR-0047); the user primarily reviews finished
  work — results, diffs, evidence — in an inbox-style surface rather than
  watching execution.
- **Event-driven**: standing rules (triggers, schedules, watched folders,
  service notices) cause agents to act; proactive reports arrive in the same
  review surface. The user manages the rule set and the outcome stream, not
  processes. This mode is recorded as the designated direction, not a current
  obligation: no runtime or service contract owns rule storage or triggers
  yet, so renderer conformance only requires keeping the review surface
  unified until that owning contract lands.

All three modes share one review surface for outcomes and one approval
mechanism for consequential actions, so the user has a single place to trust.

### D4: Permission is the UX of mounting

Host file access is expressed as grants per ADR-0050. In the UX: dragging a
folder to an agent, picking it in a system dialog, or approving an agent's
access request all create a grant; mount and bind are side effects the user
never names. A single Permissions surface lists active grants by label and
scope; revoking unmounts. Raw host paths never appear — only grant labels —
which matches what Alan OS is allowed to see.

### D5: The interaction model does not invent a universal home

There is no Alan Home, `workspace_home`, or other product object that owns or
aggregates agents, work, services, and permissions. The interaction model
defines how concrete file-backed objects are presented when reached; it does
not force every renderer into one start screen or change the current
terminal-first macOS default. Shell Space, Tab, and pane state remains
presentation structure, not Alan OS identity.

A future unified entry surface may be specified only when implemented
workflows establish its concrete contents and owning file surfaces. Until
then, adding a home ContentKind or changing default manifest semantics would
create presentation structure without an authoritative domain object.

### D6: The vocabulary rule

Default UI copy names user objects: agent, conversation, work, result,
folder, permission, service, rule. It must not name OS objects: mount,
namespace, fid, descriptor, `/proc`, tape, rollout (as a word — the concept
renders as "evidence" or "history"). OS vocabulary is allowed only in the
Files layer, power-user surfaces, debugging views, and documentation. This
rule is reviewable per screen and belongs in UI conformance checks.

### D7: Completed work requires durable discovery

Background-servant interaction is not complete if outcomes disappear with a
Process or Alan OS Host boot. A background dispatch therefore sets
`runtime_overrides.durability_required` in its `/proc/clone` document. After
listing the current `rollout_id` values and receiving the pending PID, the
renderer acknowledges the dispatch only when
`/agent/rollouts/<rollout-id>` exposes valid first-record metadata whose ID
was absent from the pre-spawn listing and whose `process_path` is
`/proc/<pid>`. The listing excludes an older retained Rollout whose PID path
was reused after Host restart without introducing a Boot identity. If the
Process exits before matching new evidence is discoverable, the dispatch fails
instead of silently accepting an in-memory Agent Machine.

Completed outcomes from accepted background dispatches and their retained
evidence references remain discoverable from retained Rollouts after Process
exit and Host restart. Best-effort foreground conversation may continue
without a Rollout, but receives no durable-review guarantee.

History is only a read-only discovery view over Rollouts, not another durable
entity. It has no independent ID, record lifecycle, persistent index, or
relationship identity. A renderer reads the Agent Runtime Service's Rollout
history surface through its Alan OS attachment; it never scans System Store
backing and never persists a private results database.

Rollout terminal completion and namespace discovery belong to the prerequisite
`expose-agent-rollout-history` change. That prerequisite also owns the
file-visible strict-durability request and Rollout correlation handshake. This
interaction-model change owns only the user-facing obligation to dispatch and
present work through those files.

## Risks / Trade-offs

- **Three layers is more design surface than one chat window.** Mitigation:
  the layers are one truth with three renderings; the Work layer ships first
  and the Files layer starts as an inspector, not a full file manager.
- **Event-driven UX without runtime machinery risks spec drift.** Mitigation:
  this change pins only the user-visible contract (rules surface, shared
  review inbox, approvals); the runtime change that implements triggers must
  conform to it.
- **Vocabulary rule can feel constraining to developer-users.** Accepted:
  advanced personal users are the audience; the Files layer preserves full
  fidelity for anyone who wants it.
- **No unified start screen means renderers may emphasize different entry
  paths.** Accepted: a speculative home would add a content kind and
  persistence contract without owning any truth. Shared interaction rules
  apply once a concrete agent, result, permission, service, or file surface is
  opened.
- **Strict durability can reject background dispatch when its Rollout cannot
  be created.** Accepted: explicit failure is preferable to acknowledging work
  whose promised outcome cannot be reviewed.

## Open Questions

- Exact name of the unified review surface (Inbox / Results / Activity) —
  copy-level, resolved in the macOS implementation change.
