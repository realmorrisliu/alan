# Proposal

## Why

The terminal product is the only Alan surface, yet it still buffers whole
assistant responses before showing them, hides usage and cost, has input-freeze and
echo bugs, ships Anthropic as a first-party provider while missing the providers
the daily workflow needs, and tests terminal behavior through a host-specific
harness. ADR-0061 settles Alan as standard-terminal-native with exactly four
first-party providers. This change is milestone M1 of the personal daily-use order:
the foundation every later milestone builds on.

## What Changes

- Stream assistant text and thinking incrementally from the generation file to the
  renderer, without busy-wait polling, while preserving tape reconciliation.
- Publish a bounded usage snapshot (turn and session tokens, context window,
  optional cost) under the Agent Process `machine/ui` surface.
- Let an invocation run ephemerally (`--ephemeral`) without reading or overwriting
  the root rollout pointer, and bind an explicit starting directory (`--cwd`).
- Make the first-party provider set exactly `magpie`, OpenAI Responses/Codex,
  Google Cloud Vertex AI and OpenRouter; keep Anthropic Messages as a protocol
  adapter for `q` provider packages.
- Fix the verified TUI/CLI defects (modal quit freeze, secret echo, leaked
  Ctrl/Alt characters, idle Esc interrupt, `/model` flow, mid-word Markdown wrap,
  false startup warnings, panic terminal restore, thinking presentation).
- Verify terminal behavior with a real Unix PTY and a terminal-state model; no
  terminal-host harness is required or retained.
- Settle docs and OpenSpec hygiene: terminal-neutral wording, ADR-0061, archive of
  delivered and retired plans, frozen deferred research.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `provider-connection-contract`: Exactly four first-party providers; Anthropic is a protocol adapter.
- `rust-inline-tui`: Incremental output and thinking, safe modal exit, idle Esc, secret entry and real-PTY verification.
- `agent-runtime-ui-file-surfaces`: Usage snapshot surface.
- `alan-os-host-lifecycle`: Ephemeral invocations do not touch the root rollout pointer.

## Impact

Owners stay as they are: Agent Execution Engine publishes generation and usage,
AgentFS owns the files, `crates/llm` owns provider wire adapters, Connection
Service owns profile metadata and Host adapters own credentials, the terminal UI
renders files. No new Process type, no host detection, no second permission
engine. Existing Connection profiles keep working; only new-profile defaults
change. Depends on ADR-0061; unblocks the trust-posture and extension-hub changes.
