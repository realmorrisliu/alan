---
name: Workspace Inspect
description: |
  Delegate bounded read-only inspection work to a fresh workspace reader.

  Use this when:
  - the task targets a different local workspace or repo than the current runtime
  - the work is read-only: inspect, search, summarize, compare, or explain
  - the parent should route into a fresh child runtime instead of pretending it switched workspaces

metadata:
  short-description: Launch a read-only workspace reader
  tags: [workspace, delegation, inspection, read-only]
capabilities:
  required_tools: [invoke_delegated_skill]
---

# Workspace Inspect

This first-party package is the parent-facing entry for read-only work in a
different local workspace.

## Working Model

1. Keep the current runtime as the steward for routing, selection, and result integration.
2. Use this package when the target work belongs in another local workspace.
3. Launch a fresh child runtime with an explicit `workspace_root` and, when useful, a narrower nested `cwd`.
4. Keep the delegated task bounded to read-only inspection, search, synthesis, or comparison.
5. Return a concise summary and any important residual uncertainty to the parent.

## Workspace-Reader Expectations

The workspace reader should:

1. inspect only the requested local workspace,
2. use read-only tools,
3. avoid edits, verification writes, or publish actions,
4. summarize findings with direct file references when available,
5. return control when the task expands beyond bounded inspection.

## Package Resources

- Use the package-local child launch target `workspace-reader`.
- Treat this as a read-only package. If the task turns into editing or verification, hand control back to the parent so it can choose a different delegated path.
