# Disposition — 2026-10-09

Proposed from the user's request to optimize cache governance and use only `alan` instead of separate `alan`/`alan-dev` product identities. The user authorized implementation as an active goal on 2026-10-09. Implementation proceeds in the dedicated `codex/unify-installation-cache-20261009` worktree; actual installation and personal-store adoption are separate operational actions and have not been performed.

Apply this change as one coherent single-product direction, with the independently verifiable slices in tasks.md. Existing stable/dev behavior remains the shipped contract until the owning deltas are implemented and merged. Preserve ADR-0054's terminal-first product boundary, ADR-0056's independent invocation lifecycle, and the explicit-recovery boundary of `unify-agent-command-input`.

Legacy store selection is an explicit migration-time decision, not a new runtime channel. Never infer that stable or dev contains the user's preferred credentials or durable history. Old sources remain untouched by default.
