## Why

Alan's target architecture makes everything a file: namespaces, mounts,
`/proc`, `/agent`, `/srv`, and the aP protocol are the system substrate. That
substrate is an excellent API, but it is not a user interaction model. Alan's
primary audience is advanced personal users, not operators who think in mounts
and namespaces. If the default experience asks users to understand "enter an
OS, start a shell, mount things with commands", the file-native architecture
becomes a barrier instead of a capability.

Today the UX contract is only implied: ADR-0046/0047 fix renderer discipline,
ADR-0050 defines grant-based host file access, and
`macos-shell-ui-ux-conformance` defines visual treatment — but no spec defines
how a user actually relates to Alan: what objects they manipulate, which
interaction modes exist, and where OS concepts are allowed to surface. Without
that contract, the macOS client, the TUI, and future Alan Apps will each invent
their own answer, and OS vocabulary will leak into the default UI.

## What Changes

- Define the Alan Interaction Model as a durable product contract: the file
  system is the API, never the default UI. Users manipulate intent, agents,
  work results, folders, grants, and services — never mounts, fids,
  namespaces, or PIDs.
- Define three disclosure layers over one shared truth: **Intent** (state a
  goal, an agent works), **Work** (agent file surfaces rendered as native
  affordances — conversation, plan cards, approval sheets, result views),
  and **Files** (the raw namespace as an explicit inspect/program layer for
  power users). All layers are live views of the same files per ADR-0046;
  no layer owns copied state.
- Establish the interaction modes: **conversation** and **background
  servant** (agents run detached; the user primarily reviews completed work
  and evidence) are first-class obligations; **event-driven** (agents act on
  events and proactively report) is recorded as the designated third mode,
  binding on renderer hosts only once a runtime or service contract owns rule
  storage and triggers.
- Require background dispatch to use Agent Runtime Service-owned
  `/mnt/agent-runtime/clone`, request strict durability in its existing
  `AgentExecutableRequest`, and succeed only after `/agent/rollouts` exposes a
  newly discovered producing Rollout correlated to the returned PID under the
  same `/proc/host/boot_id`. A pre-spawn listing excludes retained Rollouts
  whose Process path was reused after Host restart. Renderers never use
  internal runtime metadata, scan System Store backing, or persist a private
  results database.
- Make permission the UX of mounting: giving an agent access to a host folder
  is a grant flow (drag in, file picker, or approval sheet) through Host Mount
  Service per ADR-0050; mount/bind are side effects, and revocation lives in a
  single permissions surface. `mount` commands are confined to the Files layer.
- Define the vocabulary rule: default UI copy names user objects (agent,
  conversation, folder, permission, service, result); OS vocabulary (mount,
  namespace, fid, `/proc`, tape) is confined to the Files layer, power-user
  surfaces, and documentation.

## Capabilities

### New Capabilities

- `alan-interaction-model`: The product-level interaction contract — disclosure
  layers, interaction modes, grant-as-permission UX, and the vocabulary rule
  for all renderer hosts.

### Modified Capabilities

- `alan-renderer-host-contract`: Renderer hosts SHALL render agent and service
  file surfaces as domain-native affordances, provide the three disclosure
  layers, and keep OS vocabulary out of the default UI.

## Impact

- Normative for Alan for macOS (`clients/apple`), the Rust TUI, and future
  Alan Apps. It requires the Agent Runtime Service-owned Rollout history
  surface before a durable review surface can be implemented; it does not
  authorize renderer access to System Store backing.
- This change does not define an Alan Home, `workspace_home`, or another
  universal entry object. It leaves renderer defaults and the current
  terminal-first macOS presentation unchanged.
- No system-architecture change: ADR-0039 (shell before agent views),
  ADR-0045 (aP attachment), and ADR-0050 (Host Mount Service grants) remain
  the system truth; this change defines the UX layered on top of them.
- Existing `macos-shell-ui-ux-conformance` keeps owning visual treatment;
  this change owns interaction structure and vocabulary, and the two must not
  duplicate rules.
- Future event-driven work (triggers, schedules, proactive reports) must
  express its user-facing surface through this model; this change defines the
  UX contract only, not the runtime event machinery.
