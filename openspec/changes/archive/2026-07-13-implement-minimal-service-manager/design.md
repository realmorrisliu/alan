## Context

The dedicated Host initially retains fixed boot composition. Alan OS requires a
real Process to own service launch and supervision, plus file-native local
entry, Host Mount, and Connection owners.

## Goals / Non-Goals

**Goals:**

- Make Service Manager the only internal boot/supervision owner.
- Boot from a small system-package-owned `/lib/boot` tree.
- Prove readiness and failure through `/proc`, `/srv`, and manager-owned files.
- Create ordinary Shell Processes for local renderers.
- Centralize Host Mount and Connection authorities behind file services.

**Non-Goals:**

- General systemd semantics, user units, scripts, dynamic reload, distributed
  discovery, package manager implementation, or renderer UI.

## Decisions

### 1. Service Manager is the first Process

Host creates Kernel and starts only Service Manager. Service Manager reads
Boot Units and owns all later system Process lifecycle. Host-side fallback or
parallel supervision is forbidden.

### 2. Boot Units are bounded data

Units contain executable, descriptors/mounts, `after`, restart enum, required
flag, timeout, and published handles. They cannot execute arbitrary shell or
expand environment templates. System package transactions replace `/lib/boot`
atomically for the next boot.

### 3. Readiness is `/proc` plus `/srv`

A unit becomes ready when its Process is running and every declared handle is
published. Exit invalidates handles. Timeout terminates the stale launch before
restart. Manager status, attempts, PID, errors, degraded state, and retry
control live in its exported tree.

### 4. Restart behavior is fixed and bounded

Support `never`, `on-failure`, and `always` with bounded exponential backoff,
restart budget, and stable reset window. Required exhaustion fails boot before
readiness or degrades the running system afterward. Root Agent uses `always`
without bypassing the budget.

### 5. Local Entry Service creates Shell Processes

After Host peer authorization, the transport adapter requests a new entry. The
service creates `/bin/alan-shell` with Alan OS credentials and a Login Namespace
Template, then hands off its namespace. It owns no later Agent Process truth.

### 6. Host Mount and Connection services separate native adapters

Host Mount Service owns grants and projections; its platform adapter alone sees
raw Host paths. Connection Service owns metadata and callable LLM trees; its
platform adapter alone sees secrets/native login. Both expose requests and
results as files.

## Risks / Trade-offs

- [Service Manager becomes a general orchestration framework] → Enforce the
  minimal unit schema and reject unknown fields/features.
- [Crash loop drains machine resources] → Bound attempts/backoff and expose
  degraded state with explicit retry.
- [Host adapter becomes hidden authority] → Keep grant/profile truth in service
  trees and make adapters return bounded results only.
- [Shell disconnect kills work] → End only the Shell Process; Agent Processes
  retain independent `/proc` lifecycle.

## Migration Plan

1. Add unit schema, parser, dependency order, and manager status tree.
2. Boot existing services and Root Agent through units.
3. Add readiness/restart/degraded behavior.
4. Add Local Entry, Host Mount, and Connection services plus adapters.
5. Delete fixed Host composition and assert Host starts only Service Manager.

## Open Questions

None. Package Service implementation remains in the rewritten package change.
