# alan Documentation Index

This repository treats documentation as a small set of explicit categories:

1. Current implementation and operator guides under `docs/`.
2. Durable contracts and product specs under `openspec/specs/`.
3. In-flight spec/design/task work under `openspec/changes/`.
4. Maintainer-only operational notes under `docs/maintainer/`.
5. Validation guides and executable fixtures under `docs/harness/`.

OpenSpec is the only source of truth for spec management. Non-OpenSpec docs may
explain current implementation behavior, operator workflows, maintainer
operations, or validation runners, but they must not introduce a parallel
durable contract.

## Start Here

- [Architecture](./architecture.md)
- [Current Governance Contract](./governance_current_contract.md)
- [OpenSpec Long-Lived Specs](../openspec/specs/)
- [Active OpenSpec Changes](../openspec/changes/)
- [Legacy Spec Migration Bridge](./spec/README.md)

## Current Behavior And Guides

These are the best entry points for understanding what the repository
guarantees today.

- [Architecture](./architecture.md)
- [Current Governance Contract](./governance_current_contract.md)
- [Skills And Tools](./skills_and_tools.md)
- [Skill Authoring](./skill_authoring.md)
- [Testing Strategy](./testing_strategy.md)
- [macOS Auto Update](./macos_auto_update.md)
- [Live Provider Harness](./live_provider_harness.md)
- [Live Runtime Smoke](./live_runtime_smoke.md)
- [macOS Dev Channel Side-by-Side Smoke](./macos_dev_channel_side_by_side_smoke.md)

Important current-vs-target pairs:

- Governance today: [governance_current_contract.md](./governance_current_contract.md)
- Governance target design: [`governance-tooling-contract`](../openspec/specs/governance-tooling-contract/spec.md)
- Skill-system stable contract: [`skill-system-contract`](../openspec/specs/skill-system-contract/spec.md)
- Skill-system current implementation guide: [skills_and_tools.md](./skills_and_tools.md)

## Specs And Contracts

Use OpenSpec instead of treating `docs/spec/`, `plans/`, or
`docs/superpowers/` as specification stores:

- [OpenSpec Long-Lived Specs](../openspec/specs/)
- [Active OpenSpec Changes](../openspec/changes/)
- [Documentation Governance Spec](../openspec/specs/documentation-governance/)
- [Legacy Spec Migration Bridge](./spec/README.md)

## Validation And Harness

- [Harness Overview](./harness/README.md)
- [Harness Self-Eval](./harness/self_eval/README.md)
- [Harness KPI](./harness/metrics/kpi.md)

## Target Design Notes

Target design notes should live in OpenSpec. Legacy target docs under
`docs/spec/` are compatibility bridge pages only.
