# Proposal: shell-like terminal interaction

## Why

The first usable-agent tracer bullet is merged. Live inspection of that build
confirmed a concrete UX mismatch: Alan prints committed transcript to terminal
scrollback, but keeps a full-height Ratatui viewport with a composer pinned at
the bottom. This looks like a chat panel, not the terminal flow the product
requires.

## Scope

Define and deliver one interaction: a shell-like inline REPL over the existing
Root Agent Process.

- The prompt is shown as `alan >`; submitted input remains in terminal history.
- Agent output appears directly after the submitted line; the next `alan >`
  follows the output.
- Slash, skill, and file completion remain temporary inline candidate lists,
  not a persistent input panel.
- Errors and interruption outcomes are visible in the terminal. Pipe/one-shot
  behavior remains stdout for results, stderr for diagnostics, and a meaningful
  exit code. Ctrl-D on an empty prompt detaches the renderer without stopping
  accepted Agent work.
- The renderer continues to read and write the mounted AgentFS surfaces. It
  gains no Agent, Process, Host, launch, retry, or recovery authority.

This change does not implement desktop UI, background agents, a new shell
language, history browsing, retention-gap recovery, or a model router. It does
not alter input routing: current code sends ordinary entries and `!` entries to
the Agent, while slash commands remain local. Command routing and intent
classification are outside this presentation change.
Dynamic ChatGPT model discovery and changing the model of an already-running
Agent are tracked for the next capability slice: the current Provider catalog
is bundled/static and a model change means binding a different Connection.
The eventual picker must change the next turn's actual Connection/model, not
only its label.

## Capabilities

### New capability

- `alan-interaction-model`: terminal-first inline REPL behavior for interactive
  Alan use.

### Modified capabilities

- `rust-inline-tui`: replace the full-height bottom-composer baseline with a
  compact inline transcript and prompt; retain typed transcript, scrollback,
  input, and terminal restoration behavior.

## Acceptance

Automated tests cover inline layout, completion, visible errors, resize,
Unicode/multiline paste, interruption, detach, and scrollback. The same built
binary must pass live acceptance in an ordinary terminal and a Herdr pane:
successful response, transient completion, long-output scrollback, exit, and
reattach without replaying the request or stopping the Host.
