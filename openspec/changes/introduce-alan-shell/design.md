## Context

`alan-shell` is the first real client of the aP protocol and the surface for the
north-star milestone. Its design discipline is restraint: it must operate the
namespace generically so that "talk to an agent" is just file operations, never a
built-in agent feature. If the shell knew about agents, the uniform-tooling claim
of the Plan 9 model would be false.

## Goals / Non-Goals

**Goals:**

- A general namespace shell: list, read, write, tail, spawn over aP.
- Conversing with an agent emerges from those operations, with zero agent code.
- A minimal line-oriented stdio driver (M1) with concurrent stream tailing.

**Non-Goals:**

- No agent-specific command, mode, or `attach` sugar (deliberately omitted for
  now).
- No Ratatui rendering — that is `alan-terminal-ui`'s role, deferred (ADR-0025).
- No private session/reducer model; the shell holds no application state beyond
  the namespace.

## Decisions

Implements the client layer of [ADR-0025](../../../docs/adr/0025-target-crate-architecture.md)
and ADR-0024 D4.

- **aP-only client.** `alan-shell` depends on `alan-ap` and nothing else (no file
  server, no backend). It reads/writes files and writes `ctl`.
- **Generic builtins.** list/walk, `cat` (open+read), `echo >`/write, `tail`
  (blocking watch with offset), `spawn`. Control of any process/agent is writing
  to its `ctl`.
- **No agent knowledge, no sugar.** Conversation = `echo > <pid>/io/input` plus
  `tail <pid>/io/output`. The same commands operate a compiler process.
- **Stdio first.** A line-oriented driver for M1; Ratatui rendering deferred to
  `alan-terminal-ui`.
- **Concurrent tailing.** A tailing read and stdin run as independent tasks so a
  streamed response prints while the user can still type.

## Risks / Trade-offs

- **Raw ergonomics.** Without `attach` sugar, conversing is two commands
  (`tail` + `echo`); accepted to keep the shell agent-agnostic. Sugar can be
  added later as pure composition if needed.
- **Interleaving in stdio.** Concurrent print + input may interleave in the
  line-oriented M1 driver; clean separation is a rendering concern deferred to
  `alan-terminal-ui`.

## Migration Plan

1. Land aP (`define-plan9-kernel-substrate §5`) and a namespace with `/proc`.
2. Implement `alan-shell` builtins + stdio driver; demo against an echo file
   server (M1).
3. Once `alan-agentfs` + `alan-llmfs` exist, talk to a real agent (M2) with the
   same builtins.
4. Later, `alan-terminal-ui` renders `alan-shell` (Ratatui).
