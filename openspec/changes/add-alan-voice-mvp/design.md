## Context

The desired UX is still a compact press-hold-release interaction: recording
starts on hold, release begins recognition, Escape cancels, Local Mode does not
upload audio, and Cloud Mode is explicit. The old fixed-command controller is
too limited for free-form recognition and intent proposals.

The architectural correction is at the execution boundary. Alan Voice is a
host-backed capability under the canonical `alan-app-service-integration`
contract: platform audio and speech APIs may stay inside Alan for macOS, but
Alan OS clients operate capture and results through an aP tree. Typed
`VoiceIntent` remains a Voice Service domain document, not a Kernel primitive
or global command bus.

## Goals / Non-Goals

**Goals:**

- Fast, keyboard-first Hold to Talk with reliable cancellation.
- Default local recognition with no silent upload; explicit cloud recognition.
- Typed, reviewable intent proposals with descriptor-bounded targets.
- File-native execution paths shared by UI, Tools, and Agent Processes.
- Compact native feedback and clear permission repair.

**Non-Goals:**

- Always-on listening, voice calls, wake words, or a Siri replacement.
- Replacing the normal terminal or text composer.
- Making raw audio, Apple framework objects, or cloud credentials visible to
  Agent Processes by default.
- A generic Alan-wide intent registry or host control plane.
- Full voice navigation or streaming transcript editing in V1.

## Decisions

### 1. Voice Service owns a mounted capture tree

The service posts `/srv/voice` and mounts at `/mnt/voice`:

```text
/mnt/voice/
├── config                 # mode, locale, provider reference; no secret
├── status
├── captures/
│   ├── clone
│   ├── events
│   └── <capture-id>/
│       ├── status
│       ├── transcript
│       ├── intent
│       ├── target
│       ├── result
│       ├── events
│       └── ctl            # start/stop/cancel/commit
└── drafts/
    └── <intent-id>/...
```

Opening `captures/clone` allocates a capture object. The owning `ctl` controls
recording lifecycle. Transcript and intent are service-produced documents;
events are append-only and offset-resumable. `commit` applies an already
reviewable intent only when its target and required rights are still valid.

### 2. Recognition mode is separate from execution authority

Local Mode uses Apple on-device recognition when available and never silently
falls back to cloud upload. Cloud Mode requires explicit provider selection,
credential availability, and disclosure. Credentials stay in the host secret
store. Both modes produce the same provider-neutral transcript/intent files.

Recognition success does not grant execution rights. The intent can affect only
targets represented by descriptors or mounted trees available to the committing
client/service path.

### 3. VoiceIntent is an app-domain proposal document

The document contains transcript, normalized text, intent kind, target
descriptor/path, confidence, safety class, proposed operation, and review state.
First-phase kinds remain capture, agent request, task, search, and app command.
It is not a universal Alan OS intent type.

Ambiguous, low-confidence, or state-changing intents remain under `drafts/` or
the capture record until reviewed. Cancellation removes/terminally closes the
proposal before any target mutation.

### 4. Execution resolves to native file/process operations

- Capture writes a whole document into an authorized app/workspace capture tree.
- Agent request writes `io/input` to an existing selected Agent Process or opens
  bounded descriptors and spawns an Agent Executable.
- Task and search target their owning mounted app/service trees.
- Reusable app commands run a visible `/bin` Tool or write the owning app
  document/`ctl`.

If the needed target or executable is absent from the namespace, the service
creates a safe draft or asks the user to select a target.

### 5. Alan for macOS is the Hold to Talk renderer/client

The host owns the global shortcut, microphone permission prompts, compact overlay,
and Keychain integration. It drives the same capture files another authorized
client could use. The Hold to Talk surface does not land until the Voice Service
tree and direct host file client can open, watch, and control the capture files;
if that boundary is unavailable, the native surface remains blocked rather than
using a callback path.

### 6. Legacy fixed-command recognition is removed

The old `NSSpeechRecognizer` command vocabulary is not kept as a parallel path.
Former phrases pass through the same intent document and native file/process
execution rules as all other utterances.

## Risks / Trade-offs

- [Risk] File round-trips slow Hold to Talk feedback → Mitigation: in-process aP
  fast path and immediate local status before recognition completes.
- [Risk] Intent targets stale UI context → Mitigation: target descriptors/paths
  are revalidated at commit and stale targets remain drafts.
- [Risk] Local recognition quality is weak → Mitigation: visible opt-in Cloud
  Mode, never silent upload.
- [Risk] Voice applies a destructive action → Mitigation: safety class,
  reviewable proposal, current access check, and owning governance.
- [Risk] Direct host file attachment is unavailable → Mitigation: land the Voice
  Service tree and fake-recognition fixtures first, then keep native Hold to Talk
  work blocked until it can use that tree directly.

## Migration Plan

1. Add Voice Service fixtures and capture file lifecycle with fake recognition.
2. Connect Apple local recognition and permission handling behind the adapter.
3. Add typed intent proposal/target documents and reviewable drafts.
4. Route each V1 intent to native file/process operations.
5. Add opt-in cloud recognition and host secret references.
6. Remove the fixed-command controller and verify the host has no alternate
   callback execution path.

## Open Questions

- Whether Cloud Mode V1 supports one provider or a provider plug-in set.
- Whether an existing Agent Process is chosen by explicit descriptor from the
  focused Alan surface or Alan Voice always spawns a fresh Agent Process for
  agent-request intents.
