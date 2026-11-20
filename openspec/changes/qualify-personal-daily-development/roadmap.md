# Personal daily-use delivery order

Accepted 2026-10-14; resequenced 2026-11-19 by
[ADR-0061](../../../docs/adr/0061-terminal-native-os-boundary-quartermaster-and-first-party-providers.md).
This is the single coordinating plan and the single active qualification plan;
each linked change owns its behavior. Documentation completion or local branch
commits do not substitute for PR merge, current-head CI, or measured daily-use
qualification.

## Execution order

| Milestone | Status & Delivery | Exit evidence |
| --- | --- | --- |
| M1 | Completed locally (`8c2d011d`) — [Terminal-native foundation](../deliver-terminal-native-foundation/): live streaming (output, thinking, usage surface), four first-party providers, TUI/CLI bugfixes, `--ephemeral`/`--cwd`, real-PTY tests, docs and OpenSpec hygiene | Focused engine/provider/TUI tests, real Unix PTY acceptance (`pty_e2e`), quality gate, strict OpenSpec (`1.4.1`) |
| M2 | Completed locally (`c94a9936`) — OS-boundary simplification, 7 core Root tools and standard-terminal TUI/CLI polish | No static shell-text bans or Guardian prompts inside confined OS sandboxes, mounted `AGENTS.md`/`CLAUDE.md` readable as non-authoritative knowledge, 7 core Root tools (`read_file`, `write_file`, `edit_file`, `bash`, `grep`, `glob`, `list_dir`), calm POSIX/VT TUI/CLI (`Theme`, `/man`, `/tape`, `alan about`) |
| M3 | Completed locally (`f91f35c3`) — [Explicit trust posture](../add-explicit-trust-posture/) (unblocked ahead of delegation corpus), slim [project delegation](../delegate-project-autonomy/), [Quartermaster `q` v1 extension hub](../add-quartermaster-extension-hub/) (`skill`, `mcp`, `tool`, `provider`), parallel subagents and [project continuity](../restore-project-continuity/) | Invocation-only trust posture (`Governed`/`ConfinedTrusted`/`HostAccountTrusted`), saved delegation/revocation and budgets, atomic four-kind `q` lifecycle with no installation-time authority and `anthropic-provider` package, parallel subagents, inspectable `/summary` and explicit `/resume` history selection without replay |
| M4 | Completed locally (`0e8d4c95`) — `EmbeddedAlan` V2 with Slowpage (`bca4c263`) and Project-M3 (`442db7a4`) integration | Clean V2 embedding facade, writable `/mnt/<app>` mounts, `EmbedEvent` V2 streaming, `ToolPolicyHook`, multi-session isolation, feature-gated `alan-os-host` without TUI dependencies |
| M5 | Completed locally (`e6222339`) — Personal Agent foundation: memory crates cleanup, pure-Rust BM25+CJK recall, `alan memory` & `alan workflow` CLIs, `personal-notes-mcp` package, and `alanworks.app` ADR-0061 alignment | Consolidated `alan-knowledge`/`alan-memfs` recall + revisions, secret-scrubbed BM25+CJK recall across domain-scoped Memory Stores, scheduled workflows over `alan -p --ephemeral --json`, confined stdio `personal-notes-mcp`, zero Herdr references on `website/` |
| M6 | Active branch milestone — UX/OAuth/Prompts/OpenSpec cleanup: model-family system prompts & static cacheable prompt prefixes, `machine/ctl` session reset/rollback/`Shift+Tab` trust toggle, first-run TUI `/login` & `alan login` + smart cwd auto-mount + honest unconfigured boot, `/help` viewport ghosting fix & `/new`/`/fork`, [`scripts/check-embed-footprint.sh`](../../../scripts/check-embed-footprint.sh), and OpenSpec cleanup | Focused prompt/TUI/OAuth/CLI tests + real PTY E2E, minimal `alan-os-host` embed footprint check passing, `add-cognitive-model-routing` archived without sync, `qualify-agent-input-routing` parked in place, five M1–M5 changes ready for post-merge `sync + archive`, full quality gate + strict OpenSpec + current-head CI before user merge |
| Qualification | Active qualification plan (owned by this change; formal sample starts after M1–M6 merge/install) | >=50 real tasks over two consecutive weeks across Alan and `realmorrisliu.com`, >=90% without rescue, interruption/reliability gates, Codex comparison (full-trust runs reported in a separate cohort) |
| Deferred | Automatic input routing (`qualify-agent-input-routing` parked in place) and general mixed-Machine work (`2026-11-20-add-cognitive-model-routing` archived without sync) | Frozen deferred research outside M1–M6; requires an explicit new planning decision and standard-terminal (POSIX/VT) acceptance plan |

## Rules adopted by ADR-0061

- Standard-terminal-native: acceptance uses ordinary POSIX/VT terminals and a real
  Unix PTY; no Herdr or other terminal-host harness, detection or integration is
  required.
- OS-boundary security: inside a confined sandbox the OS and mount grants are the
  boundary. No static shell-text bans or Guardian prompts for contained effects;
  uncontained effects still escalate, and the pre-ADR-0061 Guardian shadow-corpus
  classifier gate is pruned. Mounted `AGENTS.md`/`CLAUDE.md` are readable
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

The five M1–M5 OpenSpec changes (`deliver-terminal-native-foundation`,
`add-quartermaster-extension-hub`, `add-explicit-trust-posture`,
`restore-project-continuity`, and `delegate-project-autonomy`) retain their
checked local evidence on `alan_ux_tui_evaluation` and stay open until remaining
docs/live checks, current-head CI, and user merge to `main`, after which they are
ready for post-merge `sync + archive`.

`qualify-agent-input-routing` remains in place under `openspec/changes/` solely as
a `deferred-research / parked` directory because `scripts/harness/collect_routing_shadow_clients.py`,
`scripts/harness/score_routing_shadow.py`, and `crates/tui/src/file_backed/shadow_admission_tests.rs`
read its pinned corpus and budget JSON artifacts. `add-cognitive-model-routing` is
archived at `openspec/changes/archive/2026-11-20-add-cognitive-model-routing/`
without syncing deferred research into canonical specs.

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
are synced and archived; six retired desktop-era plans and deferred
`2026-11-20-add-cognitive-model-routing` are archived without sync.

Background/always-on root lifetime, multi-device entry, broad proactive permission
learning and full mixed-Machine expansion remain out of scope for M1–M4; scheduled
MCP workflows are scoped to M5's Personal Agent foundation. Preserve existing
Process/Memory Store ownership; no new global Project or Conversation executor.
