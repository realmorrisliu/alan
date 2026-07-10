## 1. Voice Service Tree

- [ ] 1.1 Add Voice Service aP fixtures and
  `/mnt/voice/{config,status,captures,drafts}` file semantics.
- [ ] 1.2 Implement capture allocation, snapshot files, offset-resumable events,
  and `start|stop|cancel|commit` owning `ctl` transitions.
- [ ] 1.3 Post `/srv/voice`, mount `/mnt/voice`, and verify filtered-handle and
  access-right behavior.

## 2. Host Recognition

- [ ] 2.1 Connect Apple local recognition behind the service adapter with no
  cloud upload and explicit unavailable states.
- [ ] 2.2 Add opt-in Cloud Mode, host secret references, provider disclosure, and
  no-silent-fallback tests.
- [ ] 2.3 Keep audio, recognizer, and cloud initialization off startup paths.

## 3. Intent Proposals And Execution

- [ ] 3.1 Add typed intent proposal/target/result documents for capture, agent
  request, task, search, and app command.
- [ ] 3.2 Route committed intents to authorized file writes, owning `ctl`, `/bin`
  Tools, existing Agent Process `io/input`, or bounded Agent Executable spawn.
- [ ] 3.3 Keep ambiguous, low-confidence, destructive, stale-target, and
  unavailable-target intents as drafts until reviewed or redirected.
- [ ] 3.4 Prove cancel prevents every transcript/task/action/agent mutation.

## 4. Alan For macOS Experience

- [ ] 4.1 Implement Hold to Talk shortcut, compact feedback, keyboard
  cancellation, permission repair, mode/provider disclosure, and canonical
  `Alan Voice` copy.
- [ ] 4.2 If required, add `AlanVoiceHostCompatibilityBridge` with no bridge-owned
  state or behavior and a direct-aP deletion gate.
- [ ] 4.3 Remove the old fixed-command `NSSpeechRecognizer` controller.

## 5. Verification And Archive Readiness

- [ ] 5.1 Add file-lifecycle, mode/privacy, target-rights, Tool/Agent spawn,
  cancellation, permission, and legacy-retirement tests.
- [ ] 5.2 Run focused Apple/Alan service tests and Alan Dev visual verification.
- [ ] 5.3 Run strict validation for this change and the full OpenSpec tree.
- [ ] 5.4 Delete the host bridge after direct aP consumption lands.
- [ ] 5.5 After merge, sync `alan-voice-input` into canonical specs before
  archiving.
