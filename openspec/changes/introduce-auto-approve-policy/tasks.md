## 1. Policy posture

- [x] 1.1 Lock to a single `AutoApprove` posture: `GovernanceProfile` collapsed to one variant with tolerant deserialize (legacy `conservative`/`autonomous` configs accepted but resolve to auto-approve); `PolicyProfile` enum removed; `PolicyEngine` always builds the auto-approve ruleset (only an explicit `policy.yaml` can fine-tune individual rules — not a mode switcher)
- [x] 1.2 Conservative boundary: allow read + in-workspace write; escalate network and unknown; out-of-workspace write blocked by the existing execution path guard
- [x] 1.3 Recognize the explicitly-destructive set: deny `rm -rf /`/`mkfs`; escalate `rm -rf`, `git push`, `git reset --hard`
- [x] 1.4 No approval is remembered as a widening grant; no persistent grant is written (engine is stateless per call)
- [x] 1.5 Tests: auto/escalate/deny decision pinned per capability and destructive pattern; unknown escalates; default profile is AutoApprove

## 2. Approval UX (TUI)

- [x] 2.1 Replace composer-typed yield answers with number-key confirmation (digit selects option; hint line shows numbered choices)
- [x] 2.2 Render approval content: derived diff/command preview (slice B forms) + audit capability/reason in the escalation payload and the TUI approval surface
- [x] 2.3 Ensure no `request_id`/internal ids appear in the approval surface (done in slice A; preserved)
- [x] 2.4 Tests: digit key answers confirmation; escalation renders capability/reason/diff (`escalation_yield_renders_capability_reason_and_diff`)

## 3. Structured-input form

- [x] 3.1 `StructuredInput` yields render each question with its kind label and options
- [x] 3.2 Full interactive multi-field form (`form.rs`): field focus navigation (Tab/↑↓), per-field editing, validation on submit; opens for multi-question yields
- [x] 3.3 Tests: form edit/nav/submit (`multi_question_structured_input_opens_form_and_submits`); single/multi-select validation

## 4. Interrupt

- [x] 4.1 `Esc` interrupts at any time during a running turn (slice A; preserved under the auto posture)
- [x] 4.2 Test: interrupt issued mid-turn (slice A composer/app tests)

## 5. Verification

- [x] 5.1 `just verify` green (policy posture + single-key confirmation)
- [ ] 5.2 Manual smoke: routine edits/reads proceed silently; network/destructive prompt; structured input renders as a form
