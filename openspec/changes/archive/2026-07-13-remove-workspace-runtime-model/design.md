## Context

Current launch derives one `WorkspaceRuntimeConfig` from the host cwd and uses
workspace path identity to assemble Agent roots, Skills, Tool bindings,
sandbox roots, memory, rollouts, connection pins, and `.alan/runtime` paths.
This duplicates namespace authority and prevents one Alan OS instance from
serving unrelated Host Mounts and Processes.

## Goals / Non-Goals

**Goals:**

- Delete workspace identity from runtime, CLI, configuration, Tool, sandbox,
  memory, rollout, Agent Definition, Skill, and package contracts.
- Make Process Launch Context and namespace paths the sole execution context.
- Establish Host-selected System Store backing without exposing raw paths.
- Leave a clean contract for a system Host and Service Manager.

**Non-Goals:**

- Implement the dedicated Host process, Service Manager, macOS attachment, or
  package management.
- Preserve workspace commands, aliases, overlays, or compatibility readers.
- Silently delete user-authored files.

## Decisions

### 1. Replace workspace config with Process Launch Context

Runtime construction accepts namespace/descriptors/credentials/cwd rather than
workspace id/root. `cwd` is always an Alan OS path. Tool execution and native
sandbox projection derive from mounted Host Mount grants. Keeping renamed
workspace fields was rejected because it would preserve dual authority.

### 2. Explicit Host Mounts are the only Host file ingress

No current directory or home directory is mounted implicitly. The transitional
CLI may explicitly request a mount of its cwd, but Kernel and engine observe
only the resulting Alan OS path and grant. Child inheritance uses namespace
clone/restriction.

### 3. System Store replaces Alan home and directory-local runtime state

Platform code selects a channel-specific backing root. Each owning store uses a
subtree; no monolithic state file or raw backing mount is exposed. Live Process
state remains ephemeral. This temporary host-path resolver moves into the
dedicated Host in the next change.

### 4. Agent and Skill inputs are explicit

Agent Definitions and Skills enter Process launch by descriptor or installed
Alan OS reference. Host-directory overlay search and `--agent` selection are
deleted. Ordinary Agent Processes are launched inside Alan Shell.

### 5. Cleanup is classified by ownership

Recognized generated state is deleted. Connection metadata gets one
migrate-verify-delete pass. Possibly authored content is reported and can be
explicitly imported; the new runtime never scans it. Rollback restores code,
not deleted generated state or a compatibility reader.

## Risks / Trade-offs

- [Breaking existing CLI/scripts] → Fail unknown removed commands clearly and
  document Alan Shell/Host Mount replacements without hidden aliases.
- [Authored content mistaken for generated state] → Delete only exact owned
  paths and require explicit confirmation for authored roots.
- [Sandbox and namespace drift] → Derive both from the same Host Mount grant
  object and add a cross-surface regression test.
- [Package change implements stale assumptions] → Keep it blocked until its
  proposal, design, specs, and tasks are rewritten.

## Migration Plan

1. Add Process Launch Context and System Store path ownership.
2. Convert runtime callers and Tool/sandbox derivation.
3. Convert Agent, Skill, memory, rollout, and connection metadata owners.
4. Remove workspace CLI/registry/config and implicit directory discovery.
5. Run classified cleanup and verify no retired symbols or paths remain.

## Open Questions

None. Host process and service-owned implementations belong to the next two
changes.
