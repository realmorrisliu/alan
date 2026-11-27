---
name: alan9
section: 7
title: "ALAN9(7)"
whatis: "alan9 - the agent operating system beneath Alan: namespace, Processes, services"
order: 2
---

## NAME

alan9 - the agent operating system beneath Alan

## DESCRIPTION

alan9 is the system boundary formed by alan9 Kernel, File-Server Services,
Service Manager, the Root Agent Process, Agent Runtime Service, hosts and app
integration conventions.

alan9 Kernel owns the namespace and mounts, paths, files, descriptors, access
rights, credentials, the Process table, and the synthetic `/proc` and `/srv`
devices. It depends only on aP, Alan's byte-oriented file-service protocol.

Everything a Process can reach is in its namespace. Anything not bound there is
structurally absent rather than denied by a side channel.

## STANDARD NAMESPACE

```text
/proc   Process table; /proc/<pid> per Process; /proc/clone spawns
/agent  AgentFS: agent view of each Agent Process; /agent/root
/srv    Service Handle Registry: mountable handles posted by services
/bin    executables: Tools, Agent Executables, /bin/q
/lib    manifests and packages: /lib/exec, /lib/skill, /lib/pkg, /lib/boot
/man    manuals: /man/1, /man/skill/<name>
/mnt    mounted services and Host Mounts: /mnt/llm, /mnt/<id>
```

A model-callable Tool is exactly a visible `/bin/<tool>` with a valid manifest
at `/lib/exec/<tool>/manifest`. Skills are manual-like packages under
`/lib/skill/<name>`; they are passed by descriptor and grant nothing.

## OWNERSHIP

```text
Process                   PID, parent, credentials, descriptors, lifecycle, exit
Agent Machine             tape and transition-local state
AgentFS                   agent IO, requests, actions, machine files
rollout / checkpoint      durable execution evidence
Memory Stores, handoffs   continuity across Agent Processes
/srv handles              service discovery
```

An agent is an ordinary Process whose `/agent/<pid>` view follows the Agent
Process file layout. There is no second Kernel Process type. `/proc/<pid>` is
lifecycle truth; `/agent/<pid>` is the agent view. Live PIDs, descriptors and
namespaces are ephemeral.

## SERVICE MANAGER

Each invocation creates a Kernel and starts one Service Manager Process. It is
the sole owner of later system service and Root Agent Process lifecycle.

Service Manager reads Boot Units from `/lib/boot`. A unit declares executable,
descriptors and mounts, ordering, a required flag, timeout, a restart enum and
published handles. Arbitrary shell, environment templating, user units and
dynamic reload are rejected.

A File-Server Service is ready only while its Process runs in `/proc` and every
declared handle is published in `/srv`. Restarts follow a bounded budget.

## SERVICE HANDLE REGISTRY

`/srv` is where a running file server posts a mountable handle, for example
`/srv/package` (Package Service) or `/srv/llm` (LLM Connections, served at
`/mnt/llm`). Service state lives in the service's own tree, never in `/srv`.

## MEMORY STORES

Memory Stores are file trees owned by storage-backed file servers. A Process
reaches one only through an explicit path or descriptor such as `/memory`. Raw
backing paths never enter prompts or Agent-visible files.

## HOST FILES

Host files never enter alan9 implicitly. They arrive only as explicit Host
Mounts owned by Host Mount Service, such as the auto-mounted project directory
or a mount authorized with `/project`. The OS sandbox and explicit mount grants
are the security boundary.

## STATUS

ADR-0055 accepts an Agent Machine that composes deterministic code, typed
evaluation and generation. Finite-choice evaluation, no-effect shadow advice and
one bounded read-only mixed program have shipped; general mixed transitions have
not. Current execution remains generation-based.

## SEE ALSO

alan(1), agentfs(4), q(1)
