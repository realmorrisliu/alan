# Agent Capability Descriptor Taxonomy V1

This is the first implementation taxonomy. It is deliberately smaller than the
full product vocabulary so Kernel semantic types, the compatibility Host Service
adapter, and Alan Agent Workspace projection can ship without overfitting future
UPDF, Groove Master, or supervisor-specific workflows.

## Descriptor Shape

Every descriptor should have:

- `descriptor_id`: stable OS id, for example `agent.explain`.
- `kind`: product-level capability kind.
- `summary`: human-readable purpose.
- `context_grant_requirements`: required and optional context grant fields.
- `result_contract_defaults`: default expected result fields.
- `allowed_effect_classes`: effect classes the descriptor can request.
- `default_command_risk`: initial command risk if the run proposes actions.
- `governance_notes`: auto-run, approval, and audit expectations.
- `app_language`: how apps may rename or narrow the descriptor in domain UI.

## V1 Descriptors

| Descriptor | Purpose | Context Grant Requirements | Result Contract Defaults | Effect Classes | Default Risk | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| `agent.explain` | Explain selected app, object, document, code, task, or runtime state. | App id, target object or view ref, selected range or query result, allowed reads, privacy policy. | Answer, citations, evidence, uncertainty, follow-up questions, audit summary. | `inspect`, `draft` | Low unless it requests private cross-app reads. | UPDF reading assistance maps here first. |
| `agent.summarize` | Produce a condensed summary of bounded content or activity. | App id, target object/view/task refs, content bounds, allowed reads, evidence requirement. | Summary, citations, evidence, uncertainty, optional next actions. | `inspect`, `draft` | Low. | Useful for documents, sessions, practice logs, and long agent work. |
| `agent.plan` | Create an ordered plan for bounded work without executing it. | App id, task goal, target refs, allowed reads, optional allowed command catalog, constraints. | Plan, rationale, dependencies, risks, evidence, proposed checkpoints. | `inspect`, `draft`, `propose` | Low to medium depending on proposed commands. | Alan Agent project planning and Groove Master practice planning both map here. |
| `agent.propose_commands` | Propose concrete commands or app actions for later governed execution. | App id, target refs, allowed command catalog, command argument constraints, risk policy, evidence requirement. | Proposed commands, command risk, expected effects, preconditions, rollback/recovery notes, audit summary. | `inspect`, `draft`, `modify`, `delete`, `publish`, `execute`, `cross-app` as proposals only. | Medium by default; high for destructive or opaque commands. | Descriptor proposes actions; Command Governance decides execution separately. |
| `agent.delegate` | Delegate bounded subwork to another agent capability, skill package, or app-owned worker. | App id, parent Agent Run id or task id, delegation target, allowed context subset, allowed result contract. | Child Agent Run refs, progress, artifacts, evidence, terminal outcome, audit summary. | `delegate`, plus child descriptor effects. | Medium. | Maps current child-agent/delegated-skill behavior to bounded Agent Runs. |

## Deferred Descriptors

- `agent.transform`: defer until Result Contracts for draft objects and
  reversible edits are implemented.
- `agent.remember`: defer until User Memory, System Memory, and App Memory write
  policies are split.
- `agent.act`: do not add as a generic descriptor; use typed app commands plus
  Command Governance instead.

## Implementation Order

1. Add descriptor ids and shared semantic structs in the Kernel types change.
2. Add compatibility mappings for `explain`, `summarize`, `plan`, and
   `propose_commands` in the Agent Capability Service adapter.
3. Add `delegate` after child Agent Run/task projection exists.
4. Add app-specific aliases only in app adapters, never as Kernel-only product
   language.

