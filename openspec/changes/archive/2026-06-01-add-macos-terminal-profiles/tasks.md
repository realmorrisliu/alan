## 1. Terminal Profile Model And Store

- [x] 1.1 Add Terminal Profile value types for id, title, launch kind, Unix user, custom command, default working directory, and optional presentation metadata.
- [x] 1.2 Implement the channel-scoped Terminal Profile store under macOS Application Support with missing-store login-shell fallback.
- [x] 1.3 Add profile validation for required fields, duplicate ids, unsupported launch kinds, and unavailable executables.
- [x] 1.4 Add corrupt-store quarantine and safe fallback behavior.
- [x] 1.5 Add focused tests for store loading, saving, default profile selection, validation failures, lookup, and corrupt-file recovery.

## 2. Workspace Manifest And Shell State References

- [x] 2.1 Add optional `terminal_profile_id` to Space records and terminal content restore payloads.
- [x] 2.2 Keep old workspace manifests decodable when profile fields are absent.
- [x] 2.3 Preserve profile references through workspace materialization, live snapshot sync, pinned snapshot updates, and transcript snapshot overlays.
- [x] 2.4 Ensure missing local profiles do not delete or rewrite manifest references during restore.
- [x] 2.5 Add focused workspace manifest tests for Space references, terminal content references, old-manifest compatibility, and missing-profile preservation.

## 3. Terminal Launch Resolution

- [x] 3.1 Extend terminal boot-profile resolution to accept resolved Terminal Profile context without bypassing the existing Ghostty surface path.
- [x] 3.2 Implement structured command generation for `login_shell`, `sudo_user`, `sudo_root`, and `custom_command`.
- [x] 3.3 Project non-secret Terminal Profile metadata into terminal environment and shell metadata.
- [x] 3.4 Preserve current environment override behavior where needed for development without letting it become workspace state.
- [x] 3.5 Add terminal runtime tests for each launch kind, missing-profile fallback, unavailable executable fallback, active-task startup state, and environment projection.

## 4. Shell Interactions And Automation Surfaces

- [x] 4.1 Extend Space creation, terminal tab creation, and split creation APIs to accept optional Terminal Profile overrides.
- [x] 4.2 Implement inheritance rules: new tabs use the Space profile, new splits use the current pane profile, and explicit overrides win.
- [x] 4.3 Ensure Space profile binding changes are not retroactive for existing terminal content.
- [x] 4.4 Extend shell action routing, control-plane DTOs, and App Intents with optional terminal-profile fields where creation commands need them.
- [x] 4.5 Add focused shell action and automation tests for Space binding, tab inheritance, split inheritance, explicit override, and non-retroactive binding changes.

## 5. Settings And Sidebar UI

- [x] 5.1 Add a Terminal Profiles Settings section backed by the local profile store, separate from provider Accounts.
- [x] 5.2 Add profile creation/editing controls for structured launch modes with required-field validation and sudo guidance.
- [x] 5.3 Add Space profile binding selection from compact Space UI affordances.
- [x] 5.4 Add restrained sidebar, pane, or status hints for profile identity, root identity, custom-command profiles, and missing-profile fallback.
- [x] 5.5 Ensure normal shell chrome does not expose full custom commands; keep full command visibility in Settings or explicit diagnostics.
- [x] 5.6 Add focused Settings/sidebar tests for profile listing, validation, Space binding, missing-profile display, and redaction.

## 6. Verification, Review, And Archive Readiness

- [x] 6.1 Run focused model and runtime scripts covering workspace manifests, terminal runtime service, shell action registry, shell automation seams, and settings surface.
- [x] 6.2 Run `bash clients/apple/scripts/check-shell-contracts.sh`.
- [x] 6.3 Run a dev-channel fresh relaunch smoke path to verify persisted profile references and missing-profile fallback in the running app.
- [x] 6.4 Run `openspec validate add-macos-terminal-profiles --strict`.
- [x] 6.5 Review the diff for accidental provider connection-profile changes, workspace manifest leakage of local profile definitions, and secret/custom-command exposure in normal shell chrome.
- [x] 6.6 Prepare PR slices in dependency order: model/store/manifest, launch/interactions, UI, then visual/docs polish if needed.
- [x] 6.7 After implementation is merged, sync accepted spec deltas into `openspec/specs/` before archiving.
