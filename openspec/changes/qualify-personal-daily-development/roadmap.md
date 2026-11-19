# Personal daily-use delivery order

Accepted 2026-10-14; resequenced 2026-11-19 by
[ADR-0061](../../../docs/adr/0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md).
This is the single coordinating plan; each linked change owns its behavior.
Documentation completion does not complete product qualification.

## Execution order

| Milestone | Delivery | Exit evidence |
| --- | --- | --- |
| M1 | [Terminal-native foundation](../deliver-terminal-native-foundation/): live streaming (output, thinking, usage surface), four first-party providers, TUI/CLI bugfixes, `--ephemeral`/`--cwd`, real-PTY tests, docs and OpenSpec hygiene | Focused engine/provider/TUI tests, PTY acceptance on a standard terminal, quality gate, strict validation with the CI-pinned validator, reviewed PR and current-head CI |
| M2 | OS-boundary simplification, 7 core Root tools and standard-terminal TUI/CLI polish | No static shell-text bans or Guardian spam inside confined OS sandboxes, mounted `AGENTS.md`/`CLAUDE.md` readable as non-authoritative knowledge, polished 7 core Root tools and calm standard-terminal TUI/CLI |
| M3 | [Explicit trust posture](../add-explicit-trust-posture/) (unblocked ahead of the delegation qualification corpus), slim [project delegation](../delegate-project-autonomy/), [Quartermaster `q` v1 extension hub](../add-quartermaster-extension-hub/) (`skill`, `mcp`, `tool`, `provider`), parallel subagents and [project continuity](../restore-project-continuity/) | Invocation-only trust posture and isolation distinction (separate cohort), saved delegation/revocation and budgets, atomic four-kind `q` lifecycle with no installation-time authority and Anthropic installable as a package, parallel subagents, inspectable summaries and explicit history selection without replay |
| M4 | `EmbeddedAlan` V2 with Slowpage and Project-M3 integration | Clean V2 embedding facade, strict per-app isolation and verified Slowpage / Project-M3 host integration without TUI dependencies |
| M5 | Personal Agent foundation: memory crates cleanup, FTS5/CJK recall and scheduled MCP workflows | Consolidated Memory Store crates, verified FTS5/CJK recall over explicit stores, governed scheduled MCP workflows with clear Process ownership |
| Qualification | Two-week personal daily-use qualification (owned by this change) | >=50 real tasks, >=90% without rescue, interruption/reliability gates, Codex comparison (full-trust runs reported in a separate cohort) |
| Deferred | Automatic input routing and general mixed-Machine work (`qualify-agent-input-routing`, `add-cognitive-model-routing`) | Frozen deferred research; needs an explicit new planning decision |

## Rules adopted by ADR-0061

- Standard-terminal-native: acceptance uses ordinary POSIX/VT terminals and a real
  Unix PTY; no terminal-host harness, detection or integration is required.
- OS-boundary security: inside a confined sandbox the OS and mount grants are the
  boundary. No static shell-text bans or Guardian prompts for contained effects;
  uncontained effects still escalate. Mounted `AGENTS.md`/`CLAUDE.md` are readable
  non-authoritative knowledge.
- Quartermaster `q` v1 is the one extension hub for `skill`, `mcp`, `tool` and
  `provider` packages; installation grants no authority.
- First-party providers are exactly `magpie`, OpenAI Responses/Codex, Google Cloud
  Vertex AI and OpenRouter; Anthropic is a protocol adapter and `q` package.

## Notes

Trust posture no longer waits for the full delegation corpus; its overlapping deltas
rebase onto whichever change syncs first, never applied in reverse. Full-trust
delivery has its own cohort and must not delay or inflate the default-mode
readiness result.

Observed UI defects are fixed in small owned changes throughout M1–M5. Do not start
another broad UI redesign. Preserve Linux regression; expand Node/TypeScript or
Python environments only for observed task needs. The website provides a real
TypeScript/pnpm task source without requiring Linux first-release parity.

Historical estimates were 2–4 weeks to a personal candidate and 4–8 weeks to a
default assistant under sustained work. They are planning estimates, not promises
or substitutes for the minimum two-week measured qualification.

A supervised self-development milestone is five independently reviewed small Alan
changes, including correction of review findings. Report it separately; it does
not prove reliable delegation or unattended general bootstrap. Root fallback cannot
qualify a failed child. No self-development runs start solely from this plan.

Delivered and retired history: `embed-alan-runtime` and `unify-agent-command-input`
are synced and archived; six retired desktop-era plans are archived without sync.

Background/always-on root lifetime, multi-device entry, broad proactive permission
learning and full mixed-Machine expansion remain out of scope for M1–M4; scheduled
MCP workflows are scoped to M5's Personal Agent foundation. Preserve existing
Process/Memory Store ownership; no new global Project or Conversation executor.
