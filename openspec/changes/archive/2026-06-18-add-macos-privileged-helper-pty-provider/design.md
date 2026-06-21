## Context

Alan for macOS now has a usable Managed Users surface, but the privileged path
is still the first implementation: `osascript` prompts apply account and
sudoers changes, and readiness is ultimately tied to `sudo -n -iu <target>`.
That model proved brittle in practice and is not the right long-term permission
boundary for a terminal-first app.

The accepted direction is to move privileged local-system effects into a signed
helper service. The normal Alan app remains the UI, terminal workspace, and
renderer host. The helper owns only the privileged operations that require root:
Managed User account diagnosis/repair, legacy Alan sudoers cleanup, and
managed-user PTY child-process supervision.

This change also depends on `add-macos-alan-owned-pty-runtime`. Ghostty cannot
be treated as the managed-user process owner. The helper-backed terminal path
requires Alan-owned PTY handles first, plus the Ghostty fork work that lets
Ghostty attach to an externally owned PTY endpoint.

## Goals / Non-Goals

**Goals:**

- Define the final macOS privileged architecture for Alan-managed terminal
  users.
- Add a channel-scoped privileged helper contract for stable and dev installs.
- Remove Alan-managed sudoers as the Managed User runtime mechanism.
- Keep all privileged helper APIs typed, declarative, and account scoped.
- Launch helper-backed Managed User terminals through Alan-owned PTY runtime
  handles, with Ghostty acting as renderer/protocol adapter.
- Preserve `sudo_user` as an operator-managed manual Terminal Profile mode while
  introducing `managed_user` for Alan-managed accounts.
- Make Settings expose helper install/update/invalid states and Managed User
  repair states without requiring a password prompt for each account repair once
  the helper is healthy.

**Non-Goals:**

- Do not add a generic root command executor, shell script runner, or arbitrary
  executable launcher to the helper.
- Do not keep a sudoers fallback for helper-backed Managed Users.
- Do not make Touch ID part of the root-authority model. Touch ID may later be
  used as app-level confirmation for destructive UI, but it is not required for
  helper privilege.
- Do not make Alan a general macOS account manager. Ordinary macOS users remain
  outside Alan unless they carry Alan-managed account ownership state.
- Do not claim terminal process continuity across app quit. Helper-owned
  managed PTY sessions are tied to the requesting client connection unless a
  later daemon-continuity feature changes that contract.

## Decisions

1. **Use a signed privileged helper as the only Managed User authority.**

   The helper runs as a launchd-managed root service and accepts requests from
   the matching Alan install channel only. Stable and dev builds use distinct
   helper labels, Mach service names, bundle identifiers, and data roots so
   Alan Dev cannot mutate stable helper state and stable Alan cannot talk to the
   dev helper.

   Alternative considered: keep `osascript` plus sudoers as the main path and
   add a helper only for failed repairs. That keeps the current failure mode in
   the product and makes the security boundary harder to audit.

2. **Expose declarative helper operations, not command execution.**

   The helper API should be limited to typed operations:
   `helperStatus`, `diagnoseManagedUser`, `applyManagedUserPlan`,
   `startManagedUserPTY`, `terminatePTY`, `removeManagedUserIntegration`, and an
   optional destructive `deleteManagedUser`.

   The helper must reject raw shell text, raw sudoers content, arbitrary command
   paths, credential persistence, and broad filesystem mutation requests. The
   app sends structured account identifiers and desired state; the helper owns
   the root implementation details.

   Alternative considered: pass pre-rendered shell scripts from the app to a
   privileged runner. That is flexible, but it recreates the same string-based
   privilege problem behind a different transport.

3. **Separate helper installation authorization from normal Managed User work.**

   Settings owns helper status and install/update/uninstall actions. Those
   actions require system administrator authorization. After a helper is
   installed, signed, current, and healthy, normal Managed User create, repair,
   verify, and terminal launch requests should not trigger per-step password
   prompts.

   Alternative considered: prompt on every account create or repair. That
   matches the current short-term behavior, but it produces a worse product
   experience and does not improve the helper's API boundary.

4. **Make helper diagnosis the source of truth for Managed User state.**

   The app may cache UI summaries, but readiness must come from helper-backed
   diagnosis and verification. The helper reports states such as
   `helperNotInstalled`, `helperOutdated`, `helperSignatureInvalid`,
   `accountMissing`, `homeMissing`, `shellInvalid`, `accountNotAlanManaged`,
   `legacySudoersPresent`, `ptySpawnFailed`, and
   `destructiveConfirmationRequired`.

   Alan must not silently take over ordinary macOS accounts. A matching Unix
   name is not enough; the helper needs Alan-managed ownership evidence before
   treating an existing account as a Managed User. Home paths remain fixed to
   `/Users/<account>`, and the initial shell allowlist is `/bin/zsh`.

   Alternative considered: infer managed state from a matching Terminal Profile
   or old sudoers file. That can misclassify accounts the user created manually.

5. **Make the helper a managed-user PTY provider.**

   For `managed_user` profiles, Alan's PTY runtime asks the helper to start a
   managed-user PTY. The helper validates the client identity, account
   ownership, request shape, shell allowlist, and channel, then allocates the
   PTY, forks the child, drops to the target uid/gid/groups, execs the login
   shell, tracks the process group, and returns only the PTY endpoint/session
   metadata needed by Alan.

   Alan owns UI state, terminal input projection, resize requests, transcript
   rendering, and the Ghostty renderer attachment. The helper owns root-only
   child-process setup, signal delivery, reaping, and cleanup. Client disconnect
   closes helper sessions tied to that XPC connection.

   Alternative considered: have the app enter the managed user by invoking
   `sudo -iu <target>` inside an ordinary terminal. That requires sudoers or an
   interactive password path and keeps terminal readiness dependent on a shell
   command instead of a typed runtime handle.

6. **Add `managed_user` Terminal Profiles and keep `sudo_user` manual.**

   A helper-backed Managed User owns a read-only Terminal Profile with launch
   mode `managed_user(account)`. `sudo_user` remains available for manual,
   operator-managed profiles and continues to mean structured `/usr/bin/sudo`
   startup with any password prompt occurring inside the terminal.

   Alternative considered: reuse `sudo_user` for managed accounts and switch
   the implementation under the hood. That hides the security boundary and makes
   it too easy to reintroduce sudoers fallback.

7. **Treat old Alan sudoers state as legacy cleanup only.**

   Existing Alan-owned sudoers files matching the deterministic
   `/etc/sudoers.d/alan-terminal-<gui>-to-<target>` marker can be diagnosed as
   `legacySudoersPresent` and removed by the helper after verifying the exact
   Alan-owned contents. Non-Alan sudoers files are never removed by generic
   repair. Legacy sudoers presence is not a readiness path for helper-backed
   Managed Users.

   Alternative considered: continue accepting legacy sudoers as ready. That
   would make migration easy but preserves the old privilege mechanism
   indefinitely.

8. **Keep helper logs and errors sanitized.**

   Helper logs should include operation id, channel, account, high-level status,
   and sanitized error codes. They must not include terminal transcript,
   credentials, raw command payloads, or full privileged scripts. Terminal
   content stays in the PTY stream owned by the terminal runtime path.

   Alternative considered: log full helper request payloads for easier
   debugging. That risks leaking sensitive terminal or account material into
   privileged logs.

## Risks / Trade-offs

- **[Risk] Helper installation and update adds signing complexity.** ->
  Require explicit status states, focused signing validation, and stable/dev
  channel isolation tests before Managed User helper operations are considered
  available.
- **[Risk] The helper-backed PTY path depends on Ghostty external-PTY support.**
  -> Sequence this change after `add-macos-alan-owned-pty-runtime`; do not
  implement a temporary sudoers runtime fallback.
- **[Risk] Existing manually created macOS users can look like desired Managed
  Users.** -> Require Alan-managed ownership evidence before repair or terminal
  launch, and surface `accountNotAlanManaged` instead of taking over.
- **[Risk] Root helper API can grow into an unsafe escape hatch.** -> Keep API
  request/response DTOs typed, forbid raw commands, and add contract checks that
  reject arbitrary executable launch or sudoers editing.
- **[Risk] Legacy sudoers cleanup can remove user-owned state by mistake.** ->
  Only remove deterministic Alan-owned paths after verifying exact Alan marker
  content; otherwise report conflict and leave the file untouched.

## Migration Plan

1. Complete `add-macos-alan-owned-pty-runtime` first, including Ghostty external
   PTY attachment in the Alan-maintained Ghostty fork.
2. Add helper packaging, channel identities, status checks, install/update/
   uninstall UX, and fake-helper test seams.
3. Move Managed User diagnose/create/repair from `osascript` and sudoers into
   helper-backed typed operations.
4. Add `managed_user` Terminal Profile launch resolution and route that launch
   mode through the helper PTY provider.
5. Migrate Managed User readiness to helper diagnosis plus helper PTY spawn
   smoke verification.
6. Diagnose old Alan-owned sudoers entries as legacy state and remove them only
   through helper-verified cleanup.
7. Remove Managed User sudoers runtime fallback and add contract checks that
   prevent it from returning.

Rollback before release is to revert the helper-backed Managed User change set
and keep the current shipped Managed User behavior. After release, rollback
requires publishing a new app/helper pair that disables helper-backed Managed
User creation and leaves existing Unix accounts/homes intact; it must not
silently recreate sudoers fallback.

## Open Questions

- The exact Apple service-registration API should be chosen during
  implementation based on the active deployment target and signing constraints.
  This does not change the helper boundary or typed API contract.
- Exact helper data-root names and Mach service labels should be finalized when
  packaging identities are added, but they must remain stable/dev isolated.
