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
| M1 | Merged on `main` (`v0.1.0`; synced & archived at [2026-11-21-deliver-terminal-native-foundation](../archive/2026-11-21-deliver-terminal-native-foundation/)) — Terminal-native foundation: live streaming (output, thinking, usage surface), four first-party providers, TUI/CLI bugfixes, `--ephemeral`/`--cwd`, real-PTY tests, docs and OpenSpec hygiene | Focused engine/provider/TUI tests, real Unix PTY acceptance (`pty_e2e`), quality gate, strict OpenSpec (`1.4.1`) |
| M2 | Merged on `main` (`v0.1.0`) — OS-boundary simplification, 7 core Root tools and standard-terminal TUI/CLI polish | No static shell-text bans or Guardian prompts inside confined OS sandboxes, mounted `AGENTS.md`/`CLAUDE.md` readable as non-authoritative knowledge, 7 core Root tools (`read_file`, `write_file`, `edit_file`, `bash`, `grep`, `glob`, `list_dir`), calm POSIX/VT TUI/CLI (`Theme`, `/man`, `/tape`, `alan about`) |
| M3 | Merged on `main` (`v0.1.0`; synced & archived at [2026-11-21-add-explicit-trust-posture](../archive/2026-11-21-add-explicit-trust-posture/), [2026-11-21-delegate-project-autonomy](../archive/2026-11-21-delegate-project-autonomy/), [2026-11-21-add-quartermaster-extension-hub](../archive/2026-11-21-add-quartermaster-extension-hub/), and [2026-11-21-restore-project-continuity](../archive/2026-11-21-restore-project-continuity/)) — Explicit trust posture, slim project delegation, Quartermaster `q` v1 extension hub (`skill`, `mcp`, `tool`, `provider`), parallel subagents and project continuity | Invocation-only trust posture (`Governed`/`ConfinedTrusted`/`HostAccountTrusted`), saved delegation/revocation and budgets, atomic four-kind `q` lifecycle with no installation-time authority and `anthropic-provider` package, parallel subagents, inspectable `/summary` and explicit `/resume` history selection without replay |
| M4 | Merged on `main` (`v0.1.0`) — `EmbeddedAlan` V2 with Slowpage (`bca4c263`) and Project-M3 (`442db7a4`) integration | Clean V2 embedding facade, writable `/mnt/<app>` mounts, `EmbedEvent` V2 streaming, `ToolPolicyHook`, multi-session isolation, feature-gated `alan-os-host` without TUI dependencies |
| M5 | Merged on `main` (`v0.1.0`) — Personal Agent foundation: memory crates cleanup, pure-Rust BM25+CJK recall, `alan memory` & `alan workflow` CLIs, `personal-notes-mcp` package, and `alanworks.app` ADR-0061 alignment | Consolidated `alan-knowledge`/`alan-memfs` recall + revisions, secret-scrubbed BM25+CJK recall across domain-scoped Memory Stores, scheduled workflows over `alan -p --ephemeral --json`, confined stdio `personal-notes-mcp`, zero Herdr references on `website/` |
| M6 | Merged on `main` (`v0.1.0`) — UX/OAuth/Prompts/OpenSpec cleanup: model-family system prompts & static cacheable prompt prefixes, `machine/ctl` session reset/rollback/`Shift+Tab` trust toggle, first-run TUI `/login` & `alan login` + smart cwd auto-mount + honest unconfigured boot, `/help` viewport ghosting fix & `/new`/`/fork`, [`scripts/check-embed-footprint.sh`](../../../scripts/check-embed-footprint.sh), and OpenSpec cleanup | Focused prompt/TUI/OAuth/CLI tests + real PTY E2E, minimal `alan-os-host` embed footprint check passing, `add-cognitive-model-routing` archived without sync, `qualify-agent-input-routing` parked in place, five M1–M5 changes synced into canonical `openspec/specs/` and archived under `openspec/changes/archive/2026-11-21-*` |
| M7 | Merged on `main` (`v0.1.0`–`v0.1.10`; includes `#1133`, `#1134`, `#1164`, `#1169`, `#1170`; synced & archived at [2026-11-23-enable-self-iteration-loop](../archive/2026-11-23-enable-self-iteration-loop/), [2026-11-25-complete-quartermaster-plugin-lifecycle](../archive/2026-11-25-complete-quartermaster-plugin-lifecycle/), and [2026-11-25-add-personal-capability-extensions](../archive/2026-11-25-add-personal-capability-extensions/)) — Standalone releases (`v0.1.0`–`v0.1.10`), self-iteration loop, Quartermaster plugin lifecycle, and personal capability extensions | Tagged releases through `v0.1.10` on `main`, `fileserver` kind (`githubfs` + `gdrivefs`), `SkillsOnly` vs `Active` Root package bindings, `alan workflow watch`, community multi-skill intake, `alan-arena` read-only skill, governed `tool` packages (`gemini-api`, `colab-compute`), live `request_service` hot-mounting, canonical `openspec/specs/` synced, and changes archived |
| Qualification | Active milestone (owned by this change; formal sample starts on merged `v0.1.2+`, continuing on `v0.1.11`–`v0.1.12-beta`; preflight live dogfooding ran 2026-11-25, formal 2-week `>=50` sample underway) | >=50 real tasks over two consecutive weeks across Alan and active TS/`pnpm` web repos (`thinksmart.systems`, `alan/website`, optional `realmorrisliu.com`), >=90% without rescue, interruption/reliability gates, Codex comparison (full-trust runs reported in a separate cohort) |
| M8 | Merged on `main` (`#1167` M8.1, `#1176` M8.2, `#1177` M8.3; synced & archived at [2026-11-25-add-project-workstreams](../archive/2026-11-25-add-project-workstreams/) via `#1178`) — Project Workstreams Ledger (`ADR-0062`), in-instance async concurrency, `routefs` cross-stream handoff, TUI `/streams` & `/focus`, tiered write isolation, `alan-memfs` delta commits & `/summary rollback`, and guarded Stateful Cron Streams (`alan workflow draft`) | M8.1–M8.3 unit/integration suites + `workstream_pty_test` passing on `main`, canonical `openspec/specs/` synced and archived |
| Next-Wave Workspace & Arena | Merged on `main` (`#1172` proposal, `#1177` implementation; synced & archived at [2026-11-25-add-cow-workspaces-and-executable-arena](../archive/2026-11-25-add-cow-workspaces-and-executable-arena/) via `#1178`); Track B ([Issue #1171](https://github.com/thinksmart-systems/alan/issues/1171)) closed & merged on `main` via `#1180` | Native OS CoW workspaces (`clonefile(2)` / `FICLONE` with `git worktree` fallback) upgrading M8 Tier-3 write isolation, `/mnt/branch` (`alan-branchfs`) coordination, confined `mode = "executable"` `alan-arena`, `cow_branch_arena_e2e` passing on `main`, and Track B zero-root macOS loopback NFSv3 bridge (`#1180`) verified with live `/sbin/mount_nfs` and `sandbox-exec` `(deny network*)` |
| M9 (`v0.1.11`) | Merged on `main` (`v0.1.11`; `#1180`–`#1186`, `#1188`–`#1189`; synced & archived at [2026-11-26-add-first-class-decisions-and-smart-approval](../archive/2026-11-26-add-first-class-decisions-and-smart-approval/)) — Track B zero-root macOS loopback NFSv3 bridge (`#1180`, closes `#1171`), Sign in with ChatGPT (`#1181`), 2-row TUI status bar + live budget projection + `Ctrl+T` panel + modal mouse (`#1182`), first-class calibrated Decisions API (`#1183`) & System 1 smart approval (`ConfinedSmart` / `--smart`, `#1184`), capability-probed Kitty/iTerm2 inline image rendering (`#1185`), `alanworks.app` Plan 9 archive redesign (`#1186`), dogfooding polish (`#1188`), and `v0.1.11` release (`#1189`) | 16/16–17/17 CI green across `#1180`–`#1189`, live macOS `mount_nfs` + `sandbox-exec` verification, real Unix PTY tests (`pty_e2e_quit`, status bar, modal mouse, Kitty/iTerm2 capability probing), canonical `openspec/specs/` synced and archived, tagged `v0.1.11` release published |
| Wave 2 | Merged on `main` (`#1197` Change 1, `#1200` Change 2; synced & archived at [2026-11-28-harden-ap-multi-writer-and-actor-substrate](../archive/2026-11-28-harden-ap-multi-writer-and-actor-substrate/) and [2026-11-28-add-event-driven-process-supervision](../archive/2026-11-28-add-event-driven-process-supervision/)) — [ADR-0063](../../../docs/adr/0063-actor-fileserver-multi-writer-and-event-supervision.md): mount-time `AgentActorView`, `OpenMode::WriteExclusive` leases, `ActorFileServer` + `RouteFs` canary, `ProcFs` per-PID serialization, `/proc/<pid>/wait`, `ExecSpec.link_parent`, generation-tracked `SupervisedHandle`, event-driven `ServiceManager` & engine child supervision; `AgentMachine` `gen_statem` deferred | 16/16 CI green across `#1197` and `#1200`, `multi_writer_e2e` and `event_driven_supervision_e2e` passing on `main`, canonical `openspec/specs/` synced and archived, closes `#1187` and `#1190` |
| Deferred | Automatic input routing (`qualify-agent-input-routing` parked in place) and general mixed-Machine work (`2026-11-20-add-cognitive-model-routing` archived without sync) | Frozen deferred research outside M1–M7; requires an explicit new planning decision and standard-terminal (POSIX/VT) acceptance plan |

## Rules adopted by ADR-0061

- Standard-terminal-native: acceptance uses ordinary POSIX/VT terminals and a real
  Unix PTY; no Herdr or other terminal-host harness, detection or integration is
  required.
- OS-boundary security: inside a confined sandbox the OS and mount grants are the
  boundary. No static shell-text bans or Guardian prompts for contained effects;
  uncontained effects still escalate, and the pre-ADR-0061 Guardian shadow-corpus
  classifier gate is pruned. Mounted `AGENTS.md`/`CLAUDE.md` are readable
  non-authoritative knowledge.
- Quartermaster `q` is the one extension hub for `skill`, `mcp`, `tool`,
  `provider`, and `fileserver` packages; installation grants no authority.
- First-party providers are exactly `magpie`, OpenAI Responses/Codex, Google Cloud
  Vertex AI and OpenRouter; Anthropic is a protocol adapter and `q` package.

## Notes

Trust posture no longer waits for the full delegation corpus; its overlapping deltas
rebase onto whichever change syncs first, never applied in reverse. Full-trust
delivery has its own cohort and must not delay or inflate the default-mode
readiness result.

The five M1–M5 OpenSpec changes (`deliver-terminal-native-foundation`,
`add-quartermaster-extension-hub`, `delegate-project-autonomy`,
`add-explicit-trust-posture`, and `restore-project-continuity`) were merged on
`main` (`v0.1.0`–`v0.1.2`), synced into canonical `openspec/specs/` in dependency
order, and archived on `2026-11-21` under `openspec/changes/archive/2026-11-21-*`.
Follow-on M7 capabilities through `v0.1.10` (`enable-self-iteration-loop` via
`#1133`, `complete-quartermaster-plugin-lifecycle` via `#1134`, and
`add-personal-capability-extensions` via `#1169`/`#1170`), M8 (`add-project-workstreams`
via `#1167`, `#1176`, `#1177`), CoW workspaces / executable arena
(`add-cow-workspaces-and-executable-arena` via `#1172`, `#1177`, archived via `#1178`),
and M9 (`v0.1.11`, `#1180`–`#1186`, `#1188`–`#1189`, including Track B `#1171` via `#1180`
and `add-first-class-decisions-and-smart-approval` synced & archived under
`openspec/changes/archive/2026-11-26-add-first-class-decisions-and-smart-approval/`) are
likewise merged on `main`. Wave 2 ([ADR-0063](../../../docs/adr/0063-actor-fileserver-multi-writer-and-event-supervision.md),
implemented via `#1197` and `#1200`) is likewise merged on `main`, synced into canonical
`openspec/specs/`, and archived under `openspec/changes/archive/2026-11-28-*`, with its
implementation slices contributing real-task evidence toward `Qualification` (`AgentMachine`
`gen_statem` remains deferred). `Qualification` (`qualify-personal-daily-development`)
remains the single active execution milestone.

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
