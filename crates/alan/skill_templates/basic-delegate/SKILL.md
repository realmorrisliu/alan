---
name: __SKILL_NAME__
description: __SKILL_DESCRIPTION__
metadata:
  short-description: __SHORT_DESCRIPTION__
---

# __SKILL_NAME__

This package delegates execution to the package-local child agent `__SKILL_ID__`.

Use this skill when:
- __WHEN_TO_USE__

## Parent Runtime Contract

1. Keep the parent-side instructions short and stable.
2. Hand long-running or specialized work to the delegated child agent.
3. Return a bounded result to the parent runtime.
4. Move detailed material into `references/` and deterministic helpers into `scripts/`.

## Namespace Requirement Contract

Describe the mounts and `/bin` bindings the delegated task materially needs.
alan checks those requirements against the assembled child namespace before
spawn. If the task is narrowed, the child task names the withheld capability
and the parent retains responsibility; otherwise the parent uses its own path,
asks for missing input, or states the limitation. Never silently substitute
unrelated local context. The parent tape records the decision and a launched
child retains bounded requirement plus namespace-summary launch metadata.
