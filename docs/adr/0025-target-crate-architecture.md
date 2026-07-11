# Target Crate Architecture

Status: Accepted. Extends ADR-0024.

## Context

The repository should make namespace and dependency ownership structural. Each
mountable tree has one file-server owner, clients speak aP, and Alan Kernel sits
at the bottom of the dependency graph.

## Layers

```text
Layer 0  alan-ap
Layer 1  alan-kernel
Layer 2  backends: alan-agent-engine, alan-llm, alan-tools, alan-auth,
                   alan-knowledge, shell-core
Layer 3  file servers: alan-agentfs, alan-hostfs, alan-llmfs, alan-memfs,
                       alan-routefs, alan-editfs, alan-branchfs
Layer 4  clients: alan-shell, alan-terminal-ui, Alan for macOS
Layer 5  composition binary: alan
```

## Dependency laws

1. `alan-kernel` depends only on `alan-ap` among Alan crates.
2. A file-server crate depends on aP and its own backend, never Kernel internals,
   another file server, or a renderer.
3. A generic aP client depends on aP and its presentation libraries, not server
   implementation crates.
4. The composition binary may depend on all owners needed to assemble a host.

Dependency-boundary tests enforce these laws.

## Protocols and alphabets

`alan-ap` is the system file-service protocol: fids, qids, paths, byte reads and
writes, offsets, clone-via-open, errors, and the `FileServer` trait.

`alan-agent-protocol` is the Agent Execution Engine Event/Op alphabet retained
for transitions, AgentFS projection, Tools, approvals, plans, and renderer
records. It is not the Alan OS system protocol.

## Namespace ownership

| Namespace | Owner | Backend |
| --- | --- | --- |
| `/proc`, `/srv` | `alan-kernel` | Process table and service handles |
| `/agent` | `alan-agentfs` | `alan-agent-engine` |
| host directory mounts | `alan-hostfs` | host filesystem |
| `/mnt/llm` | `alan-llmfs` | `alan-llm` and connection profiles |
| `/mnt/mem` | `alan-memfs` | `alan-knowledge` |
| `/mnt/route` | `alan-routefs` | routing rules and queues |
| editable-buffer trees | `alan-editfs` | buffer state |
| branching-execution trees | `alan-branchfs` | branch state |

`/bin`, `/lib`, and `/man` may be unioned from multiple independently owned
file-server trees. Each contributor posts its own handle; Service Manager owns
composition.

## Crate roster

- `alan-ap`: aP protocol.
- `alan-kernel`: namespace, mounts, Process table, `/proc`, `/srv`.
- `alan-agent-engine`: AI Turing-machine loop.
- `alan-agentfs`: AgentFS projection and control surface.
- `alan-llm` / `alan-llmfs`: provider adapters and callable LLM Connections.
- `alan-tools`: current builtin Tool implementations.
- `alan-auth`: connection credentials and managed auth.
- `alan-knowledge` / `alan-memfs`: content-addressed knowledge and Memory Store
  tree.
- `alan-hostfs`, `alan-routefs`, `alan-editfs`, `alan-branchfs`: focused
  file-server owners.
- `alan-shell`: generic aP shell builtins.
- `alan-terminal-ui`: file-backed terminal renderer and input loop.
- `alan-shell-core` / `alan-shell-core-ffi`: platform-neutral workspace model
  and native ABI.
- `alan`: composition binary, direct CLI, and linked TUI.

Future target crates require their own accepted capability and must obey the
same dependency laws.

## Naming

- every crate starts with `alan-`;
- `alan-ap` is the file-service protocol;
- `alan-<tree>fs` names a user-space file server;
- `alan-kernel` names the namespace and Process substrate;
- file-unaware backends use functional names;
- clients use product-role names.

## Consequences

- Kernel changes least.
- file trees can be replaced or mounted independently.
- renderers remain projections over authoritative files.
- Agent Execution Engine details cannot leak into Kernel types.
- adding a system surface requires a named file-server owner and explicit
  namespace location.
