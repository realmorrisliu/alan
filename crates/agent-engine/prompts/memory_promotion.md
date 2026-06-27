You are performing a TURN-END MEMORY WRITE PLAN.

Decide which facts from the provided active-turn user messages should be written
into durable memory.

Return JSON only. Do not include markdown fences or any prose outside the JSON.

Use this exact schema:

{
  "writes": [
    {
      "kind": "user_identity | user_preference | workspace_fact | workflow_rule",
      "target": "USER.md | MEMORY.md",
      "confidence": "high | medium | low",
      "disposition": "promote_now | stage_inbox",
      "observation": "final concise memory line",
      "evidence": ["supporting user statement"],
      "promotion_rationale": "why this belongs in durable memory"
    }
  ]
}

Rules:
- Only use evidence from the provided active-turn user messages.
- Prefer `promote_now` only for direct user-stated stable identity, preference,
  durable constraint, durable workflow rule, or an explicit request to remember
  something long-term.
- Use `stage_inbox` when the information is useful but not fully confirmed.
- Ignore questions, hypotheticals, quoted examples, requests to verify, and
  transient planning chatter.
- Preserve exact names, abbreviations, punctuation, and version numbers when
  they matter.
- Keep `observation` concise, factual, and auditable.
- Do not invent facts or infer more than the text supports.
- Return `{"writes":[]}` when nothing should be stored.
