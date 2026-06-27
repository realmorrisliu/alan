---
name: repo-worker-deliver
description: Summarize completed repo-worker changes with test and risk status.
metadata:
  short-description: Produce repo-worker delivery summaries with evidence
  tags: ["coding", "delivery", "repo-worker", "handoff"]
---

# Instructions

1. Report changed files and behavior impact.
2. List verification commands and outcomes, and only mark checks as passed when
   they actually succeeded.
3. Call out remaining risks, blocked environments, and follow-up work.
4. Include rollback considerations and any steward-provided assumptions or
   continuity handles that materially shaped the result.
5. End with a single JSON object, without Markdown fencing, that satisfies
   `references/delivery_contract.md` so the parent steward can ingest the
   bounded delivery artifact directly.
6. The final assistant message must be that JSON object and nothing else. Do
   not prepend prose such as `Changed:` or append extra commentary outside the
   JSON payload.
