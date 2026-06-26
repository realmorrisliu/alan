# Target Crate Architecture (Plan 9 Model)

Status: Accepted. Extends [ADR-0024](0024-plan9-kernel-model.md) — 0024 defines
the model, 0025 defines how the code repository is organized to embody it.

## Context

ADR-0024 collapsed the kernel onto a Plan 9 substrate plus a file-layout
convention. This record fixes the durable crate structure and naming so the
repository layout mirrors the namespace and the dependency direction is enforced
structurally rather than by discipline alone.

## Organizing principle

**The repository structure is the namespace structure.** Every mountable tree in
the Plan 9 namespace is owned by exactly one crate, and dependencies flow in one
direction only: clients and file servers speak a small file protocol; the kernel
sits at the bottom and depends on almost nothing.

```
Layer 0  protocol         alan-ap (aP)       [alan-agent-protocol = legacy compat alphabet]
Layer 1  kernel           alan-kernel  (-> alan-ap only)
Layer 2  backends         alan-agent-engine  alan-llm  alan-tools  alan-auth   (file-unaware)
Layer 3  file servers     alan-agentfs  alan-llmfs  alan-binfs  alan-memfs  alan-pkgfs
Layer 4  clients          alan-shell   alan-terminal-ui (renderer)
Layer 5  binary           alan         (boot + Service Manager + CLI/daemon; may depend on all)
```

## Decisions

### D1. Three dependency laws (enforced by tests)

1. **`alan-kernel` depends only on `alan-ap`.** It does not know agent, llm,
   provider, runtime, or memory. This makes "the kernel changes least" a
   structural fact, not a hope.
2. **File servers depend on `alan-ap` plus their own backend only** — never on
   kernel internals, never on another file server, never on a client. They
   rendezvous through `/srv`, so any one is replaceable or multi-instance.
3. **Clients depend on `alan-ap` and (for renderers) external UI libraries only —
   never an internal server/backend Alan crate.** A front-end reads files, writes
   `ctl`, and watches streams; a renderer like `alan-terminal-ui` may use
   Ratatui/Crossterm, but it never links an Alan server or backend crate. The
   `dependency_boundary` test enforces "no internal server/backend Alan deps", not
   "no deps at all".

Only the `alan` binary may depend on everything, because it is the hand that
mounts everything together at boot.

These become `dependency_boundary` tests that fail the build on violation.

### D2. The protocol is `alan-ap` (aP), a standalone crate

The file-service protocol — **aP**, Alan's 9P analog (9P → aP) — is the wire
language every file server and client speaks: the `FileServer` trait
(`walk/open/read/write/stat/create/remove/clunk`), `Fid`/`Qid`, `Path`,
byte/offset `Stream`, error codes, the client handle, and the in-process
fast-path transport. It lives in its own `alan-ap` crate, not as a module inside
`alan-kernel`; the kernel is just aP's first host. The contract MUST be
wire-shaped per ADR-0024 D5 (fids, byte buffers, offsets, error codes; no borrows
or rich return types) even while v1 runs in-process.

Naming rationale (supersedes the earlier `alan-fs` name): `-fs` is reserved for
user-space *filesystems* (`alan-agentfs`, `alan-llmfs`, …), so the *protocol*
must not be called `alan-fs`. (The kernel-synthetic `/proc` and `/srv` are
rendered by `alan-kernel`, not a user-space `-fs` crate.) aP is the analog of Plan 9's 9P; because everything
is a file, aP is *the* Alan protocol, while `alan-agent-protocol` (the former
`alan-protocol`) is a demoted legacy compatibility alphabet behind `alan-agentfs`,
not the system protocol. aP is our own minimal protocol, not literal 9P
(ADR-0024 D5); a 9P gateway can be added later if external 9P tooling is wanted.

### D3. Namespace ownership map

| Namespace path | Served by | Backend |
| --- | --- | --- |
| `/proc`, `/srv` | `alan-kernel` (synthetic) | — |
| `/srv/agent-runtime` (handle); tree at `/agent` | `alan-agentfs` | `alan-agent-engine`, `alan-agent-protocol` |
| `/srv/llm` (handle); tree at `/mnt/llm/providers/<provider>` (introspect), `/mnt/llm/connections/<connection>` (callable) | `alan-llmfs` | `alan-llm` |
| `/bin`, `/lib/exec/<tool>`, `/man/1` | `alan-binfs` | `alan-tools` |
| `/lib/skill`, `/man/skill` | `alan-pkgfs` | — |
| `/mnt/mem` | `alan-memfs` | content-addressed knowledge store |
| `/srv/route` (handle); tree at `/mnt/route` | `alan-routefs` (`routefs` server) | — |

To add a tree: create one `alan-<tree>fs` crate implementing `alan-ap` and post a
handle under `/srv`. There is no other place new resource surfaces may live.

### D4. Crate roster and migration mapping

Alphabets:
- `alan-ap` — new; the file protocol contract (D2).
- `alan-agent-protocol` — rename of `alan-protocol`; the agent session Event/Op
  alphabet, kept as compatibility transport behind `alan-agentfs`.

Kernel:
- `alan-kernel` — new; created for the substrate (namespace engine, process
  table, `/proc`, `/srv`, in-process transport). There is no current `alan-kernel`
  crate to rewrite (the V1 one was removed); any compatibility code that must move
  lives today in `alan-runtime` / `alan-protocol` / `crates/alan` / `crates/tui`
  and migrates into the projection (`alan-agentfs`) or `alan-compat`, not the
  kernel.

Backends (file-unaware):
- `alan-agent-engine` — rename of `alan-runtime`; the Turing-machine loop, turn
  execution, compaction, tool orchestration. Projected by `alan-agentfs`.
- `alan-llm` — keep; provider adapters, wrapped by `alan-llmfs`.
- `alan-tools` — keep; tool implementations, wrapped by `alan-binfs`.
- `alan-auth` — keep; secret store / connection profiles.

File servers (each implements `alan-ap`):
- `alan-agentfs` — new; serves `/agent` (the projection crate of
  `introduce-alan-kernel-runtime`).
- `alan-llmfs` — new; posts a handle at `/srv/llm`, serves its tree at
  `/mnt/llm`; owns cost/metering/rate-limiting
  (ADR-0024 D6).
- `alan-binfs` — new; serves `/bin`, tool manifests, and man pages.
- `alan-memfs` — new; serves `/mnt/mem`; the durable home memory tree (D7).
- `alan-pkgfs` — new, optional; serves `/lib/skill` and `/man/skill`.
- `alan-routefs` — new; the `routefs` server posts a handle at `/srv/route` and
  serves its tree at `/mnt/route` (message routing; `add-message-routing`).

Clients (read files, write `ctl`):
- `alan-shell` — new; the shell over the namespace (`ls /agent`,
  `cat .../io/output`, `echo interrupt > ctl`, spawn).
- `alan-terminal-ui` (`crates/tui`) — keep the crate, re-role it as the Ratatui
  renderer of `alan-shell`; drop the private session/reducer model.

Transitional:
- `alan-compat` — new, temporary; holds V1 surfaces still needed during
  migration; deleted once clients are file-native.
- There is no current `alan-agent` crate (the V1 projection was removed). The
  current compatibility/runtime pieces live in `alan-runtime`, `alan-protocol`,
  `crates/alan`, and `crates/tui`; their projection logic migrates into
  `alan-agentfs` (per `introduce-alan-kernel-runtime`) with any V1 remnants going
  to `alan-compat`. The name `alan-agent` is reserved for a future optional Agent
  Workspace app client (may be deferred).

Binary:
- `alan` — keep; boots the kernel, runs Service Manager (mounts the file servers,
  posts `/srv`), and runs the CLI/daemon. The daemon's HTTP/WS becomes a
  transport adapter over the file protocol, not the canonical API.

### D5. Naming conventions

Three layers, three naming rules — the protocol, the filesystems that speak it,
and the kernel that hosts them must stay nameable apart:

1. Every crate is prefixed `alan-`.
2. **The protocol** is `alan-ap` (aP). There is exactly one; nothing else carries
   a protocol name. (`alan-agent-protocol` is a demoted legacy alphabet, not the
   protocol.)
3. **A user-space filesystem** is `alan-<tree>fs` (agentfs/llmfs/binfs/memfs/
   pkgfs/routefs). The `-fs` suffix means: implements aP (`alan-ap`), owns a
   namespace tree, posts to `/srv`. `-fs` is reserved for filesystems and MUST NOT
   name the protocol. The kernel-synthetic surfaces `/proc` and `/srv` are NOT
   user-space `-fs` servers — they are rendered by `alan-kernel` and are not
   published through `/srv`.
4. **The kernel** is `alan-kernel` (namespace + process table + mounts); it hosts
   aP but is not aP.
5. A file-unaware backend is a functional name (`alan-agent-engine`, `alan-llm`,
   `alan-tools`); no `-fs`, so it may not touch the protocol.
6. Clients are role names (`alan-shell`, `alan-terminal-ui`).

### D6. Directory layout

```
crates/
  ap/            alan-ap                  # the aP protocol (9P analog)
  kernel/        alan-kernel
  protocol/      alan-agent-protocol      # (current protocol, renamed)
  engine/        alan-agent-engine        # (current runtime, renamed)
  providers/     alan-llm
  tools/         alan-tools
  auth/          alan-auth
  servers/
    agentfs/     alan-agentfs
    llmfs/       alan-llmfs
    binfs/       alan-binfs
    memfs/       alan-memfs
    pkgfs/       alan-pkgfs
    routefs/     alan-routefs
  shell/         alan-shell
  tui/           alan-terminal-ui         # alan-shell's Ratatui renderer
  compat/        alan-compat              # transitional
  alan/          alan                     # binary
```

Role-based grouping is for readability; a flat `crates/*` is also acceptable.
The `servers/` group corresponds one-to-one with the namespace map (D3).

## Scope boundary: the terminal / Ghostty / macOS line is parked

The macOS app and its terminal substrate — `clients/apple/alan-macos`,
`alan-shell-core`, `alan-shell-core-ffi` (Ghostty-backed PTY/terminal emulation)
— are **out of scope for the Alan OS refactor** and are maintained on their own
line for now. The refactor does not touch them and must not be blocked by them.

Consequences:
- The active client layer for the refactor is `alan-shell` plus the
  `alan-terminal-ui` renderer. Reworking the macOS app into a file client (and
  dropping its "semantic view snapshot" model) is **deferred** until Alan OS is
  stable.
- `alan-shell-core` keeps its current name while parked. The naming overlap with
  the new `alan-shell` (the namespace shell) is tolerated for now; if and when the
  terminal line migrates, `alan-shell-core` should be renamed (for example
  `alan-terminal-core`) to free "shell" for the namespace shell.
- A future change will decide how the terminal line joins the file model; this
  ADR deliberately does not.

## Risks / Trade-offs

- **Over-crateification.** The file-server layer adds several small crates. This
  is accepted because each crate maps to exactly one namespace tree and one
  dependency boundary, which is worth more than crate count.
- **The kernel rewrite must not regress runtime behavior.** The engine
  (`alan-agent-engine`) stays intact as a backend; agentfs projects it, so model
  execution is unaffected by the kernel reshape.
- **Front-end migration is the expensive part, and it is parked, not solved.**
  Making any rich client file-native is an architecture change, not a mechanical
  one; the deferred terminal line is the largest instance.
