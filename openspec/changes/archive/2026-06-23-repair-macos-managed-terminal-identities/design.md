## Context

Alan for macOS already has three partial pieces of the local identity model:
Terminal Profile value types and launch resolution, Managed Terminal Account
planning and sudoers handoff helpers, and a Settings surface that summarizes
Terminal Profiles and local identity. These pieces do not yet form a usable
product flow. Settings rows for Terminal Profiles and Managed Terminal Accounts
are mostly static, and the Space menu flattens inheritance and identity by
showing `Default` beside `Login shell`.

The desired product model is narrower and clearer:

- Terminal Profiles are general local startup profiles.
- Managed Users are local terminal-only Unix accounts managed by Alan.
- A ready Managed User owns a read-only Terminal Profile.
- `Login shell` is the default terminal identity and fallback.

Provider connection profiles are out of scope for this change.

## Goals / Non-Goals

**Goals:**

- Make Settings > Terminal usable for multiple Managed Users.
- Keep Terminal Profiles and Managed Users as separate user-visible layers.
- Make managed-user-generated Terminal Profiles read-only from the profile
  editor.
- Remove the user-facing distinction between `Default` and `Login shell`.
- Keep successful Managed User creation from changing Space bindings or the
  default terminal identity automatically.
- Preserve workspace portability by storing only profile references in workspace
  manifests.

**Non-Goals:**

- Changing provider Connection Profile login, default, or pin behavior.
- Building a broad Identity Profile that owns Git, SSH, cloud, and agent state.
- Letting users edit raw sudoers text or privileged shell scripts.
- Making managed-user profiles freely editable.
- Creating a separate macOS Settings scene.

## Decisions

1. **Keep two layers: Managed Users and Terminal Profiles.**

   Managed Users own local Unix account lifecycle. Terminal Profiles own startup
   configuration. A managed user can produce a Terminal Profile, but Terminal
   Profiles can also exist for non-managed use cases such as root or custom
   command startup. This avoids turning Terminal Profiles into a special-case
   user-management feature.

   Alternative considered: make Settings only a Managed User surface. That
   would satisfy the immediate account creation flow but would hide Terminal
   Profiles even though they have other startup uses.

2. **Treat `Login shell` as the fixed default terminal identity.**

   `Login shell` is both a built-in Terminal Profile and the safe fallback.
   There is no separate user-facing global default profile. Existing serialized
   `default_profile_id` data may remain for compatibility during migration, but
   the normal no-override startup path resolves to `Login shell`.

   Alternative considered: keep a configurable global default. Current UI and
   user feedback show that this creates confusing `Default` versus `Login shell`
   semantics and makes the base terminal behavior harder to reason about.

3. **Managed User creation accepts only Unix user name and display label.**

   Alan derives the home directory (`/Users/<name>`), shell (`/bin/zsh`), hidden
   login-window setting, sudoers drop-in, verification check, and Terminal
   Profile id. This supports multiple users without exposing a full system user
   editor.

   Alternative considered: expose home, shell, hiding, and Space binding in the
   creation form. That is too much for the primary flow and increases the chance
   of unsafe or inconsistent state.

4. **Managed profiles are read-only in Terminal Profiles.**

   A profile with `managedTerminalAccountID` is maintained by Managed Users.
   Users can inspect it in Terminal Profiles, but launch kind, Unix user, home,
   and managed marker are not editable there. Repair/remove/recreate lives in
   Managed Users.

   Alternative considered: allow partial editing. Even label-only editing risks
   splitting the visible profile identity from the managed user record in V1.
   Read-only keeps the handoff trustworthy.

5. **Creating a Managed User does not bind the current Space.**

   Success adds a ready managed identity to the profile list and Space menu. The
   current Space remains unchanged until the user explicitly selects that profile
   from the Space menu.

   Alternative considered: offer a "Use in current Space" checkbox. The chosen
   V1 avoids surprise mutation and keeps creation separate from workspace
   organization.

6. **State is discovered and verified, not trusted from UI cache.**

   Settings should build Managed User status from local account lookup,
   Alan-owned sudoers state, non-interactive sudo verification, and Terminal
   Profile handoff state. Cached UI data can speed rendering but cannot mark an
   account ready by itself.

## Risks / Trade-offs

- [Risk] Removing user-facing global default profile surprises anyone who used
  it as a hidden shortcut. -> Show ready Managed Users in Space menus and keep
  explicit Space binding as the stable customization path.
- [Risk] Read-only managed profiles feel restrictive. -> Route repair/remove
  actions from the managed profile row back to Managed Users and keep non-managed
  profiles editable.
- [Risk] Multiple users increase privileged-operation complexity. -> Derive
  deterministic sudoers paths per GUI-user/target-user pair and validate each
  user independently before marking it ready.
- [Risk] Existing profile stores contain a non-login default profile. -> Treat
  this as legacy state, preserve definitions, and normalize user-facing startup
  to `Login shell` unless a Space or terminal content stores an explicit
  profile reference.
- [Risk] Settings becomes a system account manager. -> Keep V1 input to Unix
  name and display label; all privileged changes must be shown as a previewed
  Alan-owned plan.

## Migration Plan

1. Add model affordances for multiple Managed User rows, their status, and their
   generated read-only Terminal Profile relationship.
2. Update Settings > Terminal to render `Managed Users` and `Terminal Profiles`
   as separate sections with state-appropriate actions.
3. Update Terminal Profile editing to block managed profiles and route repair
   to Managed Users.
4. Update profile resolution and Space menus so unbound Spaces show and use
   `Login shell` instead of a separate `Default` entry.
5. Ensure existing workspace references and profile definitions remain
   preserved; missing/unready profiles fall back to `Login shell` at launch.
6. Verify with focused tests and a fresh Alan Dev visual smoke pass.

Rollback: the feature can fall back to existing Terminal Profile listing and
login-shell startup by ignoring Managed User actions. Existing Terminal Profile
definitions and workspace profile references remain decodable.

## Open Questions

- None. The brainstormed decisions are: multiple Managed Users, V1 form is
  Unix user name plus display label, created users do not bind current Space,
  and managed profiles are read-only.
