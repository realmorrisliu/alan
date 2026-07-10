## Why

Alan for macOS needs low-friction Hold to Talk without becoming always-on
dictation or a separate voice assistant. The previous change correctly defined
the product interaction but routed typed intents through session/runtime bridges;
the Plan 9-like design requires recognition, intent proposals, targeting, and
execution to meet Alan OS through a host-backed file-server tree.

## What Changes

- Use **Alan Voice** as the canonical feature name and **Hold to Talk** as the
  first interaction model.
- Preserve Local Mode, opt-in Cloud Mode, compact feedback, cancellation,
  permissions, privacy disclosure, and lazy initialization.
- Add a host-backed Voice Service that posts `/srv/voice` and serves
  `/mnt/voice`; Apple Speech, audio capture, cloud recognition, and host secrets
  remain behind the adapter.
- Represent capture instances, transcript, typed intent proposal, status,
  result, events, and lifecycle control as files.
- Resolve the target from descriptors and mounted app/service trees rather than
  session ids or global “current context” objects.
- Execute accepted intents through canonical file writes, owning `ctl` writes,
  `/bin` Tool execution, or Agent Executable spawn with bounded descriptors.
- Keep ambiguous or unsafe intents as reviewable proposal files; cancellation
  closes the capture without applying a mutation.
- Retire the old fixed-command `NSSpeechRecognizer` path.

## Capabilities

### New Capabilities

- `alan-voice-input`: Defines Alan Voice product behavior, host-backed Voice
  Service files, Hold to Talk lifecycle, recognition modes, typed intent
  proposals, descriptor targeting, execution, review, privacy, and legacy
  retirement.

### Modified Capabilities

None.

## Impact

- Alan for macOS owns shortcut/audio/permission presentation and acts as a file
  client over `/mnt/voice`.
- The Voice Service adapter owns capture and recognition projection while Apple
  or cloud implementation details remain private.
- Agent commands create or write an Agent Process through normal file/spawn
  semantics; task, search, capture, and app commands target their owning mounted
  trees or Tool executables.
- No new daemon session, runtime submission, typed RPC, or remote-control API is
  introduced.
