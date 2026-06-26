# Readiness Notes

## Decision

This model is ready to act as the target architecture for the next OpenSpec
alignment pass. The change id and vocabulary now use `define-agent-process-os-model`
and `agent-process-os-model`.

## Locked Decisions

- Alan Kernel process ontology is a single `Process` category; agent-ness is
  file-layout/AgentFS conformance, not a separate kernel process kind (ADR-0024
  D3).
- Root Agent Process is the root of the agent process tree, not Service Manager
  and not root permission.
- Service Manager replaces daemon as the canonical lifecycle concept.
- System services are file-server Processes that post handles under `/srv`.
- Agent Runtime Service serves AgentFS at `/agent`.
- Agent work starts by spawning Agent Executables, not by calling an Agent
  Capability API.
- Tools are executables with help, man pages, and manifests.
- Skills are manual-like packages passed by descriptor.
- Memory and policy are descriptor-passed file trees.
- Alan Shell is the primary OS interaction surface.
- Alan Agent is built in but optional.

## Open Follow-Ups

- Decide the exact first AgentFS file schemas for `status`, `ctl`, `io/events`,
  `machine/events`, requests, and actions.
- Decide the first Service Manager file schema under the canonical
  `/mnt/service` mount.
- Keep follow-up changes aligned with Agent Process, Agent Runtime Service, and
  AgentFS terminology.
- Plan the compatibility bridge from current session APIs to Agent Process file
  surfaces.
