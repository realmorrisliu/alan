---
name: agentfs
section: 4
title: "AGENTFS(4)"
whatis: "agentfs - the /agent file server: one directory per Agent Process"
order: 3
---

## NAME

agentfs - the `/agent` file server

## SYNOPSIS

```text
/agent/<pid>/
/agent/root/
```

## DESCRIPTION

AgentFS is the file server mounted at `/agent`. For each Agent Process it
unions the Kernel's generic `/proc/<pid>` layout with the agent surfaces served
by Agent Runtime Service. `/agent` is an overlay, not a second Process table.

Agent-ness is a file layout. A Process is an agent when `/agent/<pid>` carries
the overlay below; no Kernel flag is consulted.

AgentFS is the complete observable runtime state. A client that attaches late
hydrates from these files and needs no engine handle or callback.

## GENERIC LAYOUT

Every Process, agent or not:

```text
io/input       stream
io/output      stream, append-only, monotonic offsets
io/events      stream
status         file
ctl            file   interrupt, cancel, signal (Kernel-owned)
```

## AGENT OVERLAY

```text
events                       aggregate stream over the whole agent
machine/
  tape                       stream, append-only
  status                     read-only
  ctl                        runtime control: compact, rollback, ...
  evaluation                 read-only latest evaluation observation
  work                       read-only latest work result
  checkpoints/current
  ui/                        models queue activity plan thinking
                             notice usage budget skills events
requests/
  clone                      allocate a request
  events                     stream
  <id>/                      kind prompt options status response ctl
actions/
  clone                      allocate an action
  events                     stream
  help
  <id>/                      status process output result approval
context/
children/                    derived from /proc parentage
```

## CONTROL

Control is text written to a `ctl` on the object that owns its meaning. Generic
lifecycle goes to `/proc/<pid>/ctl`. Tape and checkpoint operations go to
`machine/ctl`. There is no catch-all `/agent/<pid>/ctl`, and leaf state files
such as `machine/status` and `requests/<id>/status` stay read-only.

`machine/ctl` accepts one UTF-8 record per write, up to 64 KiB, without CR, LF
or NUL.

## REQUESTS

Confirmations, approvals, credentials, selections and structured input are
directories under `requests/<id>/`. A client answers by writing `response`; the
answer commits on clunk, so it is delivered whole or not at all. Writing
`cancel` to `requests/<id>/ctl` cancels a pending request. Responses to an
answered, closed or cancelled request are rejected.

## ACTIONS

When an agent spawns a Tool, the Tool appears in `/proc` and `actions/<id>/`
records its status, Process reference, output, result and approval state. A
failed effect is recorded as failed, never as partial.

## TAPE

`machine/tape` is append-only. While the agent generates, the engine holds an
exclusive write lease: readers may tail, no other writer may splice.

## ROOT

`/agent/root` resolves to the current Root Agent Process of this invocation.
Service Manager starts Root from a Boot Unit with a system-owned Agent
Definition. If Root restarts, the alias follows the new PID and the old PID
stays terminal.

## EXAMPLES

```text
cat /agent/root/io/output
cat /agent/root/machine/ui/queue
ls  /agent/root/requests
```

## SEE ALSO

alan9(7), alan(1)
