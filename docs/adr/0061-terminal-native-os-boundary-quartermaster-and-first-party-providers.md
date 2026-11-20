# Terminal-native Alan, OS-boundary security, Quartermaster extension hub and four first-party providers

Status: accepted 2026-11-19. Amends ADR-0054 (terminal-host preference), ADR-0056
(Herdr wording) and the sequencing of ADR-0060; retains ADR-0030 (Quartermaster),
ADR-0050 (Host Mount grant ownership) and ADR-0051 (Connection Service owns
profiles). Planning and documentation only; no behavior below is claimed shipped
until its owning change lands, is reviewed and syncs into canonical specs.

Four decisions are recorded together because the M1–M5 execution order depends on
all of them.

## 1. Alan is standard-terminal-native

Alan targets ordinary POSIX/VT terminals: Ghostty, iTerm2, Kitty, WezTerm,
Terminal.app, tmux and SSH sessions. No terminal host is a runtime, acceptance or
authorization dependency. ADR-0054's preference for Herdr as the preferred host is
withdrawn; the rest of ADR-0054 stands (no desktop client, no recreated windows,
sidebar, workspace manager or terminal emulator).

Acceptance is standard-PTY based: a real Unix PTY exercised with a terminal-state
model (`openpty` + `vt100`), covering startup frame, resize reflow, bracketed
paste, wide/CJK text, Ctrl+C/Ctrl+D/Esc, modal exit and scrollback. Terminal
multiplexers (tmux) are checked as ordinary hosts. Terminal-host identifiers, pane
IDs and detection states grant no Alan identity or authority, and detecting a
specific host is never required. Historical evidence recorded through Herdr
remains valid history and is not rewritten.

Lifecycle is unchanged: each bare `alan` invocation owns one foreground instance;
a terminal-host view detach that retains the native process is distinct from actual
process exit.

## 2. Security boundary is the operating-system boundary

Inside a confined OS sandbox (Seatbelt, Linux reified namespace) the OS and the
explicit mount grants are the security boundary. Alan therefore:

- stops brittle static string bans on shell text and stops Guardian prompts for
  actions the confinement already contains;
- keeps token-aware red lines and Guardian review for effects that escape or cannot
  be contained by the active boundary (network egress beyond grants, out-of-grant
  writes, publication, privilege and control-policy changes);
- never treats model confidence, a reviewer verdict or a classifier as a new grant;
- lets Agents read `AGENTS.md` and `CLAUDE.md` inside an explicitly mounted project
  as non-authoritative knowledge. Their text can inform work like any file but can
  never widen authority, change posture, add grants or alter control policy.

Fully trusted posture remains an invocation-only, trusted-user-origin choice
(ADR-0060). It is no longer gated on the full delegation qualification corpus:
`add-explicit-trust-posture` proceeds ahead of that corpus, with its own cohort,
and it still promises no tamper-resistance against same-account code.

## 3. Quartermaster `q` v1 is the unified extension hub

Quartermaster is one extension surface with four package kinds: `skill`, `mcp`,
`tool` and `provider`. The Package Service owns install state, catalog, lifecycle
and projections from its System Store subtree; `q` is the user surface. Installed
packages are projected into the Standard Namespace under explicit descriptors.
There is no implicit Host-directory source (ADR-0052). A package gains no
authority, credential or mount by being installed; each effect still runs through
Agent Runtime governance and existing grants. MCP servers run as ordinary governed
Processes. Provider packages register protocol adapters bound to Connection
profiles; credentials stay with Host adapters (ADR-0051).

## 4. Four first-party providers; Anthropic is protocol and package

The built-in first-party provider set is exactly:

1. `magpie` — local/custom OpenAI Chat Completions-compatible router;
2. OpenAI Responses and ChatGPT/Codex;
3. Google Cloud Vertex AI;
4. OpenRouter.

Anthropic is demoted from first-party built-in to a protocol adapter
(`anthropic_messages`) that remains available to `q` provider packages. Existing
Connection profiles keep working; new-profile defaults, onboarding and catalogs
offer only the four first-party providers, with other protocols added through `q`.
The Connection Service remains the metadata owner and credentials remain with Host
adapters.

## Execution order

| Milestone | Delivery |
| --- | --- |
| M1 | Terminal-native foundation: live streaming (output, thinking, usage surface), four first-party providers, TUI/CLI bugfixes, `--ephemeral`/`--cwd`, standard-PTY tests, docs and OpenSpec hygiene |
| M2 | OS-boundary simplification, 7 core Root tools and standard-terminal TUI/CLI polish |
| M3 | Explicit trust posture (`add-explicit-trust-posture`, unblocked ahead of delegation corpus qualification), slim project delegation (`delegate-project-autonomy`), Quartermaster `q` v1 (`skill`, `mcp`, `tool`, `provider`), parallel subagents and project continuity (`restore-project-continuity`) |
| M4 | `EmbeddedAlan` V2 with Slowpage and Project-M3 integration |
| M5 | Personal Agent foundation: memory crates cleanup, FTS5/CJK recall and scheduled MCP workflows |

Owners and exit evidence live in the
[roadmap](../../openspec/changes/qualify-personal-daily-development/roadmap.md).

## Consequences

- Herdr-specific acceptance harnesses are cancelled; PTY acceptance replaces them.
- Delegation qualification no longer blocks trust posture; trust posture is
  qualified as its own cohort and never inflates the default daily-use score.
- Retired desktop-era plans are archived; cognitive and input-routing work is
  frozen deferred research behind this order.
- Package kinds beyond `skill` are added only with their owning M3 tasks; this ADR
  claims none as shipped.
