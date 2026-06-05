## 1. Presentation Boundary

- [x] 1.1 Introduce a dedicated Quick Terminal presentation controller that
  observes the quick-terminal slot and drives panel visibility without owning
  shell mutations.
- [x] 1.2 Move `NSPanel` ownership, delegate behavior, collection behavior,
  ordering, and key/focus timing into a Quick Terminal window presenter.
- [x] 1.3 Replace the Peak's full `TerminalPaneView` composition with a narrow
  Quick Terminal content view and model.
- [x] 1.4 Keep `ShellHostController` as the owner of show, hide, close, cwd,
  close guard, and promotion semantics while delegating presentation.

## 2. Runtime And Focus Sequencing

- [x] 2.1 Split panel presentation from terminal surface attachment so the
  panel can become visible before Ghostty surface setup runs.
- [x] 2.2 Add or clarify runtime registry seams for quick-terminal
  content-mount attach and focus after host view registration.
- [x] 2.3 Bound focus retries and make terminal focus best-effort after window
  visibility.
- [x] 2.4 Preserve hidden runtime state across hide/show and preserve existing
  close teardown behavior.

## 3. Persistence And Promotion

- [x] 3.1 Materialize persisted quick-terminal presentation as hidden during app
  launch, including old manifests that recorded visible presentation.
- [x] 3.2 Preserve `Open in Space` as a move of the existing runtime into a
  normal Alan tab and clear the quick-terminal slot after promotion.
- [x] 3.3 Ensure promotion releases the Peak presentation without copying,
  linking, or finalizing the terminal process.

## 4. First Implementation Verification

- [x] 4.1 Run the focused Apple build needed to prove the refactor compiles.
- [x] 4.2 Verify stable-channel launch safety without operating Alan Dev.
- [x] 4.3 Confirm the stable launch path cannot trap on invalid Peak collection
  behavior.
- [x] 4.4 Document any deferred Quick Terminal behavior verification.

Verification note: stable app launch and Quick Terminal show/hide behavior were
verified against an isolated stable `Alan.app` bundle with a temporary
Application Support directory and temporary shell-control namespace. Alan Dev
was not operated.

## 5. Follow-Up Verification Slice

- [x] 5.1 Add presentation state-machine tests for show, hide, close, attach,
  focus, and promotion transitions.
- [x] 5.2 Add an AppKit harness covering Peak panel collection behavior,
  visibility, and focus ordering.
- [x] 5.3 Add runtime attach/focus sequencing tests that prove early focus
  requests do not race host view registration.
- [x] 5.4 Run stable-channel Quick Terminal behavior verification without
  touching Alan Dev.

## 6. OpenSpec Validation

- [x] 6.1 Run `openspec validate stabilize-macos-quick-terminal-peak --type change --strict --json`.
- [x] 6.2 Run `openspec validate --all --strict --json`.
- [x] 6.3 Run `git diff --check`.
