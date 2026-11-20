# AGENTS

Workspace operating conventions for this Agent Definition.

## Process Start

Persona files (`SOUL.md`, `ROLE.md`, `USER.md`, `TOOLS.md`, `HEARTBEAT.md`) and active `Memory Store Bootstrap` files are already injected into context. Do not re-read them with tools unless you need a fresh update.

## Memory Hygiene

- Persist durable user preferences, decisions, and handoff notes only in `/memory` after clear user confirmation or explicit continuity needs.
- Keep transient turn state out of persona files (`USER.md`, `SOUL.md`).
