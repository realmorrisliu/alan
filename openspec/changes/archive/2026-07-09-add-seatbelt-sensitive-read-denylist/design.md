## Context

`refactor-sandbox-spec-input` introduced `SandboxSpec { writable_roots,
read_denylist, network }`, and `add-host-dir-file-server` made the `alan`
composition root project host mount declarations into that spec. Today the
`read_denylist` field is threaded through macOS Seatbelt profile generation, but
default specs and host-mount specs leave it empty.

`define-namespace-driven-sandbox` D3 selected broad reads plus a sensitive-read
denylist as the near-term macOS path. Seatbelt can express that directly with
`deny file-read*` rules. Landlock cannot express "allow broad reads except these
subpaths", so Linux remains write+network confined until namespace reification
provides full read isolation.

## Goals / Non-Goals

**Goals:**

- Populate default `SandboxSpec` values with a curated home-sensitive read
  denylist.
- Preserve that denylist when `alan` adds read-write host mounts to the spec.
- Make macOS Seatbelt enforce those read denies using the existing
  `read_denylist` profile input.
- Keep the platform narrative explicit: Linux carries the field but does not
  enforce sensitive-read denies today.

**Non-Goals:**

- Deny-by-default read isolation.
- Linux namespace reification or mount namespace work.
- Agent-requestable mounts through `PolicyEngine`.
- Kernel-denying workspace `.git`, `.alan`, or `.agents` protected subpaths.

## Decisions

1. **Generate the default denylist in `alan-agent-engine` at `SandboxSpec::seed`.**

   The current runtime paths already seed sandbox specs inside the agent engine.
   Putting the default there hardens both direct `Sandbox::new` users and current
   tool execution without requiring every composition root to remember a second
   helper.

   Alternative considered: generate the list only in `alan` when projecting host
   mounts. That would miss existing engine-only paths and make the hardening
   dependent on a particular host composition path.

2. **Keep host-mount projection additive.**

   `alan::host_mounts::sandbox_spec_from_host_mounts` should start from
   `SandboxSpec::seed(workspace_root)` and append canonical read-write host mount
   roots. This preserves the D4 layering rule: the declaration list is projected
   in `alan`, while `alan-kernel` remains host-path agnostic.

3. **Use a curated home-sensitive list, not existence filtering.**

   The denylist is derived from the user home directory and includes Alan home
   stores, common cloud/dev credential stores, macOS keychain and browser profile
   directories. Entries are included even if they do not exist yet; Seatbelt
   profile rendering can canonicalize existing paths and otherwise keep the
   lexical path.

4. **Do not over-claim Linux read protection.**

   Landlock currently receives `read_denylist` for signature stability, but it
   intentionally ignores the list because its allow-list model cannot express
   broad reads minus selected denies. Tests and docs should make macOS the only
   sensitive-read enforcement target in this slice.

## Risks / Trade-offs

- **A legitimate tool needs a denied home credential path** -> The access should
  be modeled as an explicit future mount or credential handoff rather than a
  broad ambient read.
- **Denying all `~/.alan` reads blocks ad hoc shell inspection of Alan config** ->
  This is intentional for tool subprocesses; Alan's own runtime reads those files
  before spawning tools and is not confined by the child Seatbelt profile.
- **Linux behavior appears weaker than macOS** -> The spec states this directly
  and keeps full read isolation assigned to the later reification change.
