---
name: plan
description: |
  Create and manage structured execution plans for complex multi-step tasks.

  Use this when:
  - Work requires more than 3 distinct steps or spans multiple subsystems
  - Steps have clear ordering or verification dependencies
  - Progress spans turns or needs to be resumed across Agent Processes
  - The user asks for a structured breakdown or milestone tracking

  Do NOT use for simple single-step tasks or quick questions.

metadata:
  short-description: Structured execution plans for complex tasks
  tags: [planning, orchestration, workflow, multi-step, incremental]
capabilities:
  required_tools: [read_file, write_file, edit_file, grep, glob]
---

# Plan Skill

Plan and track multi-step engineering work cleanly using alan9's built-in `update_plan` virtual tool and, when cross-process durability is needed, the Memory Store at `/memory`.

## Core Workflow

1. **Explore before planning**: Inspect relevant files with `read_file`, `grep`, and `glob` so every step names concrete files, symbols, and checks.
2. **Publish a concise live plan with `update_plan`**:
   - Call `update_plan` with `explanation` (short intent/context) and `items` (`[{id, content, status}]`), where `status` is one of `"pending"`, `"in_progress"`, or `"completed"`.
   - Keep at most one step `"in_progress"` at a time unless independent read/write work is genuinely executing concurrently.
   - Update `update_plan` immediately when scope changes or a step finishes.
3. **Persist cross-session plans in `/memory` (only when durable handoff is needed)**:
   - Write longer architectural checklists or milestone logs under `/memory/working/` or `/memory/topics/` using `write_file` / `edit_file`.
   - Do NOT create `.alan/plans/*.json` or ad-hoc plan artifacts inside the mounted host repository unless the user explicitly asks for repo-checked docs.
4. **Verify before marking `"completed"`**:
   - Run relevant builds, linters, or focused tests and observe them pass before flipping a step to `"completed"`.
   - Never run `git add` or `git commit` automatically as part of finishing a plan step unless the user explicitly asks you to commit.

## `update_plan` Shape

```json
{
  "explanation": "Align adapter defaults and verify cache invariants",
  "items": [
    { "id": "1", "content": "Inspect projection modules and callers", "status": "completed" },
    { "id": "2", "content": "Update request projection and add unit tests", "status": "in_progress" },
    { "id": "3", "content": "Run focused cargo test and clippy gates", "status": "pending" }
  ]
}
```
