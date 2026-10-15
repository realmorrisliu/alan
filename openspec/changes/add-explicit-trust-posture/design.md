# Design

## Context

This change depends on delegate-project-autonomy and ADR-0060. It revises the
single-posture prohibition deliberately. It is a separate delivery, not an
exception silently hidden inside reviewer code or the default-mode score.

## Goals / Non-Goals

Allow explicit uninterrupted execution. Do not claim that approval bypass grants
admin privileges, supplies credentials, creates service mount handles, or makes
arbitrary local code harmless. Do not persist the invocation-only trusted choice.

## Decisions

Use two approval postures: autonomous and fully trusted. The canonical control
representation and CLI spelling are implementation details to document only when
shipped; malformed selection must fail. Isolation is independently selected:
confined by current explicit grants, or native Host-account execution. Confined
is the default even after approval bypass is selected. Host-account execution is
available only with an additional trusted-user selection of fully trusted posture;
ordinary autonomous execution never silently degrades into it.

The trusted-user entry displays both effective posture and actual isolation before
activation. A Process cannot enable/broaden its own selection through tool output,
AgentFS writes, project files, saved delegation, summaries or inherited argv that
it manufactures. Capture user origin at the Host entry rather than trusting a
string in the model transcript. Launch explicitly carries this choice to children;
parent posture inheritance never implicitly copies resource mounts. A child can
be narrower, never broader. A new invocation and explicit durable recovery default
to autonomous/confined unless the user newly selects trusted options.

Fully trusted skips per-action allow/deny/always-human classification and Guardian.
It does not bypass input validation, Process rights, service grant protocols, OS
checks, cancellation, task budgets, once-only effect recovery or enforcement still
provided by the selected confined backend. An effect blocked by confinement returns
a clear failure; it never silently expands rights or turns into a per-action prompt.
In Host-account mode, native commands have the launching account’s real authority,
not namespace-derived isolation. Ordinary file Tools retain their explicit file
protocol semantics; no fabricated Host Mount is added to make the claim look uniform.

The trusted control API is user-only by design. With unrestricted native code,
this is not proof against same-account mutation of control/audit backing files or
another same-account process. Document the distinction in UI and evidence. Do not
claim restricted private-data access, tamperproof logging or protected control
files without an actual external OS boundary. The user explicitly accepted this
trade-off in Q13. If a user requires hard protection, choose confined execution.

Switches to a narrower mode invalidate pending reviewer/approval decisions and
pause incompatible future admission. Changing mode does not retroactively undo
completed effects; cancellation targets owned execution through the existing
Process lifecycle. Never announce a running unconfined process as newly sandboxed.
Verify posture changes during pending/queued/running work and child launches.

## Risks / Trade-offs

- User mistakes bypass for isolation → always display both actual selections.
- Repository enables bypass → accept activation only from trusted user entry.
- Host code alters local controls → explicit limitation; use external isolation when required.
- Trusted results inflate reliability → separate cohort and mode-qualified evidence.

## Migration Plan

Deploy only after the dependency and its canonical sync. Existing autonomous
configurations keep their meanings. No persisted project default enables full
trust. On rollback or unknown mode, fail rather than fall back to broad execution.
