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
Layer 0  alphabets        alan-fs            alan-agent-protocol
Layer 1  kernel           alan-kernel  (-> alan-fs only)
Layer 2  backends         alan-agent-engine  alan-llm  alan-tools  alan-auth   (file-unaware)
Layer 3  file servers     alan-agentfs  alan-llmfs  alan-binfs  alan-memfs  alan-pkgfs
Layer 4  clients          alan-shell   alan-terminal-ui (renderer)
Layer 5  binary           alan         (boot + Service Manager + CLI/daemon; may depend on all)
```

## Decisions

### D1. Three dependency laws (enforced by tests)

1. **`alan-kernel` depends only on `alan-fs`.** It does not know agent, llm,
   provider, runtime, or memory. This makes "the kernel changes least" a
   structural fact, not a hope.
2. **File servers depend on `alan-fs` plus their own backend only** — never on
   kernel internals, never on another file server, never on a client. They
   rendezvous through `/srv`, so any one is replaceable or multi-instance.
3. **Clients depend on `alan-fs` only.** A front-end reads files, writes `ctl`,
   and watches streams; it never links a server or a backend directly.

Only the `alan` binary may depend on everything, because it is the hand that
mounts everything together at boot.

These become `dependency_boundary` tests that fail the build on violation.

### D2. `alan-fs` is a standalone crate from the start

The file-server protocol contract — the `FileServer` trait
(`walk/open/read/write/stat/create/remove/clunk`), `Fid`/`Qid`, `Path`,
byte/offset `Stream`, error codes, and the in-process fast-path transport — lives
in its own `alan-fs` crate, not as a module inside `alan-kernel`. Every file
server and client depends on `alan-fs`; the kernel is just its first host. The
contract MUST be wire-shaped per ADR-0024 D5 (fids, byte buffers, offsets, error
codes; no borrows or rich return types) even while v1 runs in-process.

### D3. Namespace ownership map

| Namespace path | Served by | Backend |
| --- | --- | --- |
| `/proc`, `/srv` | `alan-kernel` (synthetic) | — |
| `/agent` | `alan-agentfs` | `alan-agent-engine`, `alan-agent-protocol` |
| `/srv/llm/<provider>` | `alan-llmfs` | `alan-llm` |
| `/bin`, `/lib/exec/<tool>`, `/man/1` | `alan-binfs` | `alan-tools` |
| `/lib/skill`, `/man/skill` | `alan-pkgfs` | — |
| `/mnt/mem` | `alan-memfs` | (storage) |

To add a tree: create one `alan-<tree>fs` crate implementing `alan-fs` and post a
handle under `/srv`. There is no other place new resource surfaces may live.

### D4. Crate roster and migration mapping

Alphabets:
- `alan-fs` — new; the file protocol contract (D2).
- `alan-agent-protocol` — rename of `alan-protocol`; the agent session Event/Op
  alphabet, kept as compatibility transport behind `alan-agentfs`.

Kernel:
- `alan-kernel` — rewrite of the current crate; namespace engine, process table,
  `/proc`, `/srv`, in-process transport. The current V1 modules
  (`agent_capability`, `descriptors`, `views`, `ledger`, `registry`,
  `invocation`) are deleted or relocated to `alan-compat`.

Backends (file-unaware):
- `alan-agent-engine` — rename of `alan-runtime`; the Turing-machine loop, turn
  execution, compaction, tool orchestration. Projected by `alan-agentfs`.
- `alan-llm` — keep; provider adapters, wrapped by `alan-llmfs`.
- `alan-tools` — keep; tool implementations, wrapped by `alan-binfs`.
- `alan-auth` — keep; secret store / connection profiles.

File servers (each implements `alan-fs`):
- `alan-agentfs` — new; serves `/agent` (the projection crate of
  `introduce-alan-kernel-runtime`).
- `alan-llmfs` — new; serves `/srv/llm/*`; owns cost/metering/rate-limiting
  (ADR-0024 D6).
- `alan-binfs` — new; serves `/bin`, tool manifests, and man pages.
- `alan-memfs` — new; serves `/mnt/mem`; the durable home memory tree (D7).
- `alan-pkgfs` — new, optional; serves `/lib/skill` and `/man/skill`.

Clients (read files, write `ctl`):
- `alan-shell` — new; the shell over the namespace (`ls /agent`,
  `cat .../io/output`, `echo interrupt > ctl`, spawn).
- `alan-terminal-ui` (`crates/tui`) — keep the crate, re-role it as the Ratatui
  renderer of `alan-shell`; drop the private session/reducer model.

Transitional:
- `alan-compat` — new, temporary; holds V1 surfaces still needed during
  migration; deleted once clients are file-native.
- The current `alan-agent` crate (a V1 projection): its projection logic moves to
  `alan-agentfs` and its V1 remnants to `alan-compat`. The name `alan-agent` is
  reserved for the optional Agent Workspace app client (may be deferred).

Binary:
- `alan` — keep; boots the kernel, runs Service Manager (mounts the file servers,
  posts `/srv`), and runs the CLI/daemon. The daemon's HTTP/WS becomes a
  transport adapter over the file protocol, not the canonical API.

### D5. Naming conventions

1. Every crate is prefixed `alan-`.
2. A file server is `alan-<tree>fs` (agentfs/llmfs/binfs/memfs/pkgfs). The `-fs`
   suffix means: implements `alan-fs`, owns a namespace tree, posts to `/srv`.
3. A file-unaware backend is a functional name (`alan-agent-engine`, `alan-llm`,
   `alan-tools`); no `-fs`, so it may not touch the file protocol.
4. Alphabets/contracts are `alan-fs` and `alan-agent-protocol`.
5. Clients are role names (`alan-shell`, `alan-terminal-ui`).

### D6. Directory layout

```
crates/
  fs/            alan-fs
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
