# Explicit delegation separates approval posture from execution isolation

Status: accepted direction, 2026-10-14, after the user confirmed Q1–Q16.
Not implemented by this documentation delivery. Refines ADR-0003/0011/0022;
retains ADR-0050 grant ownership and ADR-0055 evaluation-versus-authority.

Alan should complete ordinary delegated project work without repeated permission
prompts. The user delegates resource/action scope; Agent Runtime judges proposed
actions within it; Host adapters enforce the actual execution boundary. Scope,
approval posture and execution isolation are distinct axes. A model’s confident
judgment is not a new grant.

## Decision

The governed default remains autonomous. Explicit project delegation can cover
read/edit/build/test/local commit and bounded dependency access, and separately
identified publication targets. Ordinary push does not imply force-push or remote
deletion. New private resources, privilege and control-policy changes require a
trusted user control. Existing one-action approval never creates standing scope.

Users may explicitly save project delegation. Host Mount Service owns file grants,
identity validation, live projection and revocation; Agent Runtime owns governed
action policy. They use existing durable service stores, not model memory or a
new global Project runtime. Reuse validates identity and revocation; it does not
restore old live handles or automatically execute a previous task. Child Processes
receive explicitly passed equal or narrower capabilities.

System 1/System 2 remain work modes, not Process types or permission levels.
Reuse the no-tools Guardian with a dedicated configurable Connection and policy.
Deterministically delegated routine work does not need model review. A new reviewer
configuration requires its own no-effect qualification. Bounded main-model fallback
may resolve uncertainty; otherwise retain one pending user decision and continue
only independently authorized work. Failure is never implicit approval.

A separately delivered fully trusted posture disables per-action policy red lines
and Guardian. It is explicitly enabled by the user for one invocation only and is
not remembered by project files or recovered history. Confined execution remains
the default. A further explicit choice permits native Host-account execution;
absence of a sandbox alone never selects it. Revocation, cancellation, task budgets,
protocol validation and operating-system rights remain meaningful boundaries.

Trusted control APIs reject Agent-origin changes. Truly unrestricted same-account
code can nevertheless modify that account’s writable control/audit backing; this
mode does not promise tamperproof controls, sensitive-file isolation or sandboxing.
The user must be shown that limitation. An OS-protected boundary remains necessary
when those guarantees are required. Broader trust does not manufacture credentials,
admin privileges or aP mount handles.

Project continuity lives in evidence, Memory Stores and handoff files. Root remains
an invocation-local execution role. Authorized history selection and inspectable,
correctable/deletable summaries support explicit recovery; reading a summary does
not replay effects, revive revoked authority or activate full trust. Background
lifetime, scheduling, cross-device entry and automatic permission learning remain
outside this decision.

## Alternatives and consequences

- Always ask: simple authority handling, but repeated interruptions prevent useful
  delegation and encourage indiscriminate approval.
- Let a model grant everything: removes friction but confuses uncertain judgments
  with authority and cannot substitute for an execution boundary.
- Always unrestricted: useful in deliberately trusted environments, but not an
  appropriate implicit default for personal files and connected resources.
- Permanent global Root: unnecessary for continuity and conflicts with the chosen
  independent foreground invocation lifecycle.

This decision deliberately replaces the canonical single-mode prohibition and
blanket prohibition on explicitly saved delegation when its changes ship. Current
canonical rules remain in effect until that delivery. It does not retrospectively
claim that old defaults were wrong or that the new modes are already available.

## Owning changes

- [Project delegation and Guardian](../../openspec/changes/delegate-project-autonomy/)
- [Explicit trust posture](../../openspec/changes/add-explicit-trust-posture/)
- [Project continuity](../../openspec/changes/restore-project-continuity/)
- [Personal daily-use roadmap and qualification](../../openspec/changes/qualify-personal-daily-development/roadmap.md)
