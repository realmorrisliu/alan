---
name: plan
description: |
  Create and manage structured execution plans for complex multi-step tasks.

  Use this when:
  - Task requires more than 3 distinct steps
  - Work is expected to span multiple context windows
  - There are logical dependencies between steps
  - You need to track incremental progress
  - User asks for something complex that benefits from a plan

  Do NOT use for simple single-step tasks or quick questions.

metadata:
  short-description: Structured execution plans for complex tasks
  tags: [planning, orchestration, workflow, multi-step, incremental]
capabilities:
  required_tools: [read_file, write_file, edit_file, bash]
---

# Plan Skill

Create and manage JSON-based execution plans for complex, multi-step tasks.

## Why Plan?

Without a plan, agents tend to:
- **One-shot everything**: Try to do too much at once, exhausting context mid-work
- **Leave messy state**: Half-implemented features with no documentation
- **Declare premature completion**: See some progress and think the job is done

A structured plan forces **incremental progress** — the single most important factor for long-running agent success.

## Plan File Format

Plans are stored as JSON in `.alan/plans/`:

```
.alan/plans/
└── {plan-id}.json
```

### JSON Schema

```json
{
  "id": "plan-001",
  "title": "Human-readable task description",
  "created_at": "2026-02-25T09:00:00+08:00",
  "updated_at": "2026-02-25T10:30:00+08:00",
  "status": "in_progress",
  "current_step": 2,
  "steps": [
    {
      "id": 1,
      "description": "Set up user model and database schema",
      "status": "completed",
      "notes": "Created users table with email, password_hash fields"
    },
    {
      "id": 2,
      "description": "Implement login endpoint",
      "status": "in_progress",
      "notes": ""
    },
    {
      "id": 3,
      "description": "Add JWT token generation",
      "status": "pending",
      "notes": ""
    }
  ]
}
```

### Step Status Values

| Status        | Meaning                                              |
| ------------- | ---------------------------------------------------- |
| `pending`     | Not started yet                                      |
| `in_progress` | Currently working on this                            |
| `completed`   | Finished successfully                                |
| `blocked`     | Cannot proceed (needs user input or external action) |
| `skipped`     | Decided not to do this step                          |

## Plan Creation Protocol

### Step 1: Analyze Complexity

Determine if planning is needed:

- **No plan needed**: Simple questions, single-step tasks, quick fixes
- **Plan needed**: Multi-file changes, new features, refactoring, multi-step workflows

### Step 2: Break Down into Steps

Create 3-7 concrete, actionable steps:

**Good steps** (single action, verifiable):
- "Create database migration for users table"
- "Implement POST /api/login endpoint"
- "Add unit tests for auth module"

**Bad steps** (vague, compound, or filler):
- "Do the task" (too vague)
- "Research, design, and implement auth" (compound)
- "Think about the approach" (not actionable)

### Step 3: Write the Plan

```
write_file .alan/plans/{plan-id}.json <plan JSON>
```

### Step 4: Present to User

Show the plan before executing:

```
I've created a plan for this task:

1. ⏳ Create database migration for users table
2. ⏳ Implement POST /api/login endpoint
3. ⏳ Add JWT token generation
4. ⏳ Add unit tests for auth module
5. ⏳ Update API documentation

Shall I proceed with step 1?
```

## Incremental Execution Protocol

**This is the most critical part.** Execute ONE step at a time:

### For each step:

1. **Update status to `in_progress`**:
   ```
   edit_file .alan/plans/{plan-id}.json
   ```

2. **Do the work** for this single step

3. **Verify the work**: Run tests, check compilation, validate behavior

4. **Commit progress** (clean state):
   ```
   bash git add -A && git commit -m "plan-{id} step {n}: {description}"
   ```

5. **Update plan**: Mark step as `completed`, add notes, advance `current_step`

6. **Report progress** to user:
   ```
   ✅ Step 1: Created database migration
       Notes: Added users table with email, password_hash, created_at fields

   🔄 Starting step 2: Implement login endpoint...
   ```

### If context is running low:

- Finish the current step
- Commit and update the plan
- Stop — the next session will pick up from the plan

## Session Resume Protocol

When resuming work on an existing plan:

1. **List available plans**:
   ```
   bash ls .alan/plans/
   ```

2. **Read the active plan**:
   ```
   read_file .alan/plans/{plan-id}.json
   ```

3. **Find the next step**: First step with status `pending` or `in_progress`

4. **Verify previous work**: Run basic checks that prior steps are still working
   ```
   bash git log --oneline -5
   ```

5. **Continue execution** from the current step

## Plan Adaptation

Plans can evolve during execution:

- **Add steps**: If you discover new requirements, insert steps
- **Never delete completed steps**: They are historical record
- **Can skip steps**: Mark as `skipped` with notes explaining why
- **Update notes**: Add context as you learn more

When adapting, explain the change to the user:

```
I discovered that we also need rate limiting. Updating the plan:

1. ✅ Create database migration
2. ✅ Implement login endpoint
3. 🔄 Add JWT token generation (current)
4. ⏳ Add rate limiting middleware  ← NEW
5. ⏳ Add unit tests
6. ⏳ Update API documentation
```

## Rules

1. **One step at a time**: Never work on multiple steps simultaneously
2. **Commit after each step**: Leave the codebase in a clean, working state
3. **Update the plan JSON**: Keep it as the single source of truth for progress
4. **Don't modify completed steps**: Only add notes, never change status back
5. **Present plan before executing**: Get user buy-in on the approach
6. **Be concrete**: Each step should produce a tangible, verifiable outcome
