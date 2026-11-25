---
name: alan-arena
description: |
  Orchestrate a thin multi-provider design bakeoff or root-cause competition
  using parallel `spawn_subagent` contenders across distinct configured models
  and an independent read-only verifier. Defaults strictly to `read_only`,
  with opt-in `mode = "executable"` CoW workspace isolation behind a
  fail-closed OS sandbox gate.

  Use this when:
  - Comparing competing architectural designs, refactors, or trade-offs across models
  - Running competing root-cause diagnoses before writing a fix
  - Running an opt-in `mode = "executable"` bakeoff where contenders edit isolated CoW clones and run checks behind OS confinement
  - Delegating the multi-model comparison step of an imported community playbook (such as `pstack`)
metadata:
  short-description: Parallel multi-model bakeoff (`read_only` default, opt-in `executable` CoW mode)
  tags: [arena, multi-model, bakeoff, architecture, subagent, verifier, read-only, cow]
capabilities:
  required_tools: [read_file, grep, glob]
---

# Alan Arena (`alan-arena`)

Run an impartial multi-model comparison by dispatching parallel `spawn_subagent` contenders across distinct configured models and adjudicating their outputs through an independent `read_only: true` verifier.

By default (`mode = "read_only"` or omitted), every contender and verifier runs strictly read-only. Opting into `mode = "executable"` runs contenders in isolated Copy-on-Write (CoW) workspaces under `/mnt/branch` (`alan-branchfs`) behind a fail-closed OS sandbox gate.

## Non-Negotiable Safety & Neutrality Invariants

1. **Read-Only Default Preserved**: Unless the operator or playbook explicitly requests `mode = "executable"`, every `spawn_subagent` call in the arena MUST set `read_only: true` with zero `read_write` Host Mounts.
2. **Fail-Closed OS Sandbox Gate (`mode = "executable"` only)**: Before creating any CoW workspace or spawning any writable contender, verify the active sandbox backend is OS-enforced (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`). If the active backend is `HostMountPathGuard`, REFUSE `mode = "executable"` immediately and advise falling back to standard `read_only` `alan-arena` comparison.
3. **Narrowed Contender Isolation (`mode = "executable"` only)**: Cap parallel contenders at `max_contenders = 4`. Each contender MUST receive strictly ONE narrowed `read_write` Host Mount covering only its own CoW clone (`workspace_ref`), default-denied network, zero Host Store secrets, and a bounded `timeout_secs` / task budget. Never grant a contender the parent Host project mount or another contender's clone.
4. **Independent `read_only: true` Verifier**: In BOTH `read_only` and `executable` modes, the `arena-verifier` subagent MUST run with `read_only: true` and read-only views only—never `read_write`.
5. **Configured Models Only**: Use only distinct model identifiers actually configured in the user's active Alan runtime or explicitly specified by the user. Never fabricate unconfigured provider/model names. If only one model is configured, state that honestly and either run distinct framing roles or ask the user to configure/select additional models.
6. **One-Off Ephemeral Streams & Clean Lifecycle**: Keep arena runs stateless (`persistence: "one_off"` when named workstreams are active). In `mode = "executable"`, purge losing/discarded CoW clones immediately upon verdict, never auto-sync winning edits back to the Host without explicit confirmation + 3-way conflict check, and flush unapplied winning `.patch` + scorecard evidence to System Store on exit before deleting clone directories.

## Composition with Community Playbooks (e.g., `pstack`)

When an imported community playbook bundle is mounted under `/lib/pkg/<pkg>/` (such as `pstack`'s `how`, `architect`, or `prove-it` skills and shared `playbooks/*.md`):

- Follow the upstream playbook's phases, constraints, and problem framing verbatim without forking or rewriting its instructions.
- Hand `$alan-arena` the bounded problem statement, relevant paths, and acceptance criteria (default `mode = "read_only"`, or explicit `mode = "executable"` when isolated test verification is requested), then return the verifier's scored synthesis back into the playbook flow.

## Arena Protocol

### 1. Frame the Bounded Bakeoff Prompt & Select Mode

Before spawning any subagent, assemble:
- **Mode**: `mode = "read_only"` (default) or explicit opt-in `mode = "executable"`.
- **Problem Statement**: Exact question, bug symptom, or architectural decision.
- **Scope**: Relevant namespace paths (explicit read-only `host_mounts` for `read_only` mode, or authorized `source_grant` for `executable` CoW cloning).
- **Acceptance Criteria**: Concrete scoring axes (correctness, boundary/security adherence, simplicity, testability, operational risk).

If `mode = "executable"`:
- Verify an OS-enforced sandbox (`Seatbelt`, `LinuxReifiedNamespace`, or `Landlock`) is active. Abort cleanly before creating CoW clones if only `HostMountPathGuard` is available.
- Provision up to 4 isolated CoW clones via the Host CoW substrate (`workspace_ref` + `backing_kind`, keeping raw Host paths out of `/mnt/branch`) and fork candidate entries in `/mnt/branch/ctl`.

### 2. Dispatch Parallel Contenders

Emit a contiguous batch of 2 or more (max 4 in `executable` mode) `spawn_subagent` tool calls in one turn so Alan executes them concurrently:

- Give each contender a distinct `role` (for example `arena-contender-1`, `arena-contender-2`, `arena-contender-3`) and a distinct configured `model` override.
- In `mode = "read_only"` (default): set `read_only: true` on every call and grant at most `read_only` mounts.
- In `mode = "executable"`: set `read_only: false`, grant strictly that contender's single narrowed `read_write` CoW clone mount, keep network denied, pass zero secrets, and set bounded `timeout_secs`.
- Pass the exact same bounded problem statement, scope, and acceptance criteria to every contender, requesting concrete file/symbol citations, trade-offs, and (in `executable` mode) verified test outcomes.

### 3. Adjudicate with an Independent `read_only: true` Verifier

Once all contenders finish (and in `executable` mode, after collecting opaque `diff_summary` and `checks_summary` into `/mnt/branch`), spawn one independent `spawn_subagent` call with:
- `role: "arena-verifier"`
- `read_only: true`
- Optional distinct configured `model` override (never sharing a `read_write` workspace with contenders).
- Prompt containing ONLY the problem statement, acceptance criteria, read-only views/paths to spot-check, and neutrally labeled contender dossiers (`Contender A`, `Contender B`, ... with opaque `workspace_ref`, `diff_summary`, and `checks_summary` in `executable` mode—never raw Host paths).

Instruct the verifier to:
1. Verify factual file/symbol claims, diffs, and check outcomes read-only against the codebase/namespace.
2. Score each contender explicitly against every acceptance criterion.
3. Flag ungrounded claims, hidden coupling, security/boundary risks, or missing edge cases.
4. Recommend the strongest option (or concrete hybrid synthesis) with clear rationale.

### 4. Publish Verdict, Purge Losers & Deliver Scorecard

- In `mode = "executable"`: record Verifier scores and `select` the winner in `/mnt/branch/ctl`, **immediately discard** all losing/non-selected candidate clones so physical CoW directories are reclaimed right away, and retain the unapplied winner (`workspace_ref`) for explicit confirmation (`apply` with 3-way conflict check, unified `.patch` export, or exit-time `.patch` + scorecard flush to System Store).
- Present to Root / Operator:
  - Active mode (`read_only` or `executable` + active OS sandbox backend).
  - Which distinct configured models participated (and any skipped/unavailable models).
  - The verifier's side-by-side scorecard and verified findings (plus opaque `workspace_ref` / `diff_summary` / `checks_summary` when `executable`).
  - The recommended path and residual risks, stopping before modifying the Host project directory unless explicitly confirmed.
