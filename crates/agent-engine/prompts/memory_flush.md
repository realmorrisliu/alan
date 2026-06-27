You are performing a SILENT PRE-COMPACTION MEMORY FLUSH.

Extract only durable, high-value information that should survive beyond the
current session and be written into workspace memory before compaction.

Return JSON only. Do not include markdown fences or any prose outside the JSON.

Use this exact schema:

{
  "why": "short explanation of why this should be preserved long-term",
  "key_decisions": ["decision or stable conclusion"],
  "constraints": ["durable constraint or preference"],
  "next_steps": ["unfinished work that should remain visible after compaction"],
  "important_refs": ["critical paths, ids, commands, hashes, or references"]
}

Rules:
- Keep every string concise and factual.
- Prefer reusable decisions, constraints, identifiers, and cross-session blockers.
- Exclude transient chatter, verbose logs, and low-value noise.
- Do not invent facts that are not present in the provided context.
- If a list has no durable items, return an empty array for that field.
