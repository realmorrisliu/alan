## 1. Model And Migration

- [ ] 1.1 Add a user-facing Managed User summary model that supports multiple users with Unix user name, display label, readiness state, repair state, conflict state, and managed Terminal Profile id.
- [ ] 1.2 Derive V1 provisioning defaults from Unix user name plus display label: `/Users/<name>`, `/bin/zsh`, hidden login-window state, sudoers rule, verification command, and profile id.
- [ ] 1.3 Normalize terminal startup resolution so no explicit or Space-bound profile resolves to `Login shell` instead of a separate global default Terminal Profile.
- [ ] 1.4 Preserve existing Terminal Profile definitions and workspace `terminal_profile_id` references while treating legacy non-login default profile state as non-authoritative for unbound startup.

## 2. Managed Users Flow

- [ ] 2.1 Build Settings > Terminal Managed Users list with state-appropriate Create, Review, Repair, Verify, and Remove actions.
- [ ] 2.2 Implement the create flow with Unix user name and display label fields plus validation and duplicate/conflict handling.
- [ ] 2.3 Show a compact privileged plan preview before apply, including account, home, shell, hidden login UI, sudoers, validation, verification, and Terminal Profile handoff.
- [ ] 2.4 Apply approved plans through the existing privileged executor boundary and refresh status from discovered local state.
- [ ] 2.5 Ensure successful creation does not bind the current Space and does not change the default terminal identity from `Login shell`.

## 3. Terminal Profiles And Space Menus

- [ ] 3.1 Split Settings > Terminal into Managed Users and Terminal Profiles sections without merging provider Connection Profile concepts.
- [ ] 3.2 Mark managed-user-generated Terminal Profiles read-only and route repair/remove actions back to Managed Users.
- [ ] 3.3 Keep non-managed Terminal Profiles available for general startup profile inspection and editing.
- [ ] 3.4 Update Space profile menus so unbound Spaces show `Login shell` selected and do not show a sibling `Default` item.
- [ ] 3.5 Disable missing, partial, repairable, or conflicting Managed Users as ready Space identities and expose repair guidance.
- [ ] 3.6 Make selecting `Login shell` clear the Space `terminal_profile_id`; selecting a ready managed or non-managed profile binds that profile.

## 4. Verification

- [ ] 4.1 Extend managed account planner and state tests for multiple users, narrow input defaults, conflict handling, no auto-binding, and no default identity mutation.
- [ ] 4.2 Extend Terminal Profile tests for read-only managed profiles, editable non-managed profiles, legacy default normalization, and missing profile preservation.
- [ ] 4.3 Extend Settings model/UI tests for two-section Terminal IA, managed user statuses, no `Default` profile row, safe wording, and redaction.
- [ ] 4.4 Extend shell action/menu/runtime tests for `Login shell` unbound startup, ready managed user binding, clearing binding, and fallback on missing profile.
- [ ] 4.5 Run focused shell verification scripts, `openspec validate repair-macos-managed-terminal-identities --strict`, and `git diff --check`.
- [ ] 4.6 Run a fresh Alan Dev visual smoke for Settings and the Space Terminal Profile menu.

## 5. Review And Archive Readiness

- [ ] 5.1 Review the diff for accidental provider Connection Profile changes, workspace leakage of local user definitions, sudoers command exposure, and managed profile editability holes.
- [ ] 5.2 Sync accepted spec deltas into `openspec/specs/` after implementation merges.
- [ ] 5.3 Archive the OpenSpec change after merge and final verification.
