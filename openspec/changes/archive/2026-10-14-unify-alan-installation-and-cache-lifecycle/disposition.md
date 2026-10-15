## Closure receipt — 2026-10-14

Implementation PR #1042 is merged at `997ade6a0e27f10df4a2a54e19daf8b52d929f5a` and is an ancestor of
main `ea929481`. GitHub reinspection confirms all sixteen checks succeeded
on reviewed head `c245172aeef7f82f449305c11cbccc9a3781b1d8` and no unresolved review threads remain.
All planning artifacts are complete. This documentation-only closure synchronizes
the delivered delta into canonical specs and archives the completed change;
canonical adoption of this closure itself remains subject to its PR merge.
Existing later canonical requirements are retained. No new runtime execution,
personal-store migration, deployment or qualification result is claimed.
Earlier status entries below are historical and do not override this receipt.

The delivered product uses one executable and one product System/Host Store
pair. Earlier stable/dev and migration-not-performed statements describe the
implementation session, not the current shipped contract. Legacy migration
remains explicit; this closure performs no migration or installation.

# Disposition — 2026-10-09

Proposed from the user's request to optimize cache governance and use only `alan` instead of separate `alan`/`alan-dev` product identities. The user authorized implementation as an active goal on 2026-10-09. Implementation proceeds in the dedicated `codex/unify-installation-cache-20261009` worktree; actual installation and personal-store adoption are separate operational actions and have not been performed.

Apply this change as one coherent single-product direction, with the independently verifiable slices in tasks.md. Existing stable/dev behavior remains the shipped contract until the owning deltas are implemented and merged. Preserve ADR-0054's terminal-first product boundary, ADR-0056's independent invocation lifecycle, and the explicit-recovery boundary of `unify-agent-command-input`.

Legacy store selection is an explicit migration-time decision, not a new runtime channel. Never infer that stable or dev contains the user's preferred credentials or durable history. Old sources remain untouched by default.
