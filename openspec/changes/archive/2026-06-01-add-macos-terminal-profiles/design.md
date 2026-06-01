## Context

Alan for macOS already has a durable shell workspace model: Spaces, tabs,
splits, content containers, terminal restore snapshots, Settings, shell action
routing, and Ghostty-backed terminal startup. The current terminal startup
contract is still effectively one-dimensional: a pane launches the GUI user's
login shell, with process-level environment overrides such as
`ALAN_SHELL_BOOT_COMMAND` and `ALAN_SHELL_LOGIN_SHELL` for development.

The user workflow now needs a more explicit local identity layer:

- `morris` remains the GUI user.
- Terminal work may happen as Unix users such as `alan`, `univer`, `lab`, or
  `root`.
- Each identity can have its own home directory, SSH keys, Git config, cloud
  credentials, and command-line agent state.
- Spaces should make this identity choice visible and repeatable without
  conflating it with Alan provider `connection_profile`.

Existing constraints:

- Workspace manifests are the shell restore authority, but they must not embed
  machine-local secrets or host-specific profile definitions.
- `connection_profile` already means provider/model credential selection in the
  daemon/runtime. Terminal startup identity must use a distinct name.
- Dev and stable macOS channels have separate app support and verification
  paths.
- Terminal startup must continue through the existing Ghostty surface path and
  shell state projection rather than introducing a parallel terminal backend.

## Goals / Non-Goals

**Goals:**

- Add machine-local Terminal Profiles that define how new terminal content
  starts.
- Keep Terminal Profiles separate from Alan provider connection profiles and
  future broader identity profiles.
- Support common structured startup modes: login shell, sudo Unix user, sudo
  root, and advanced custom command.
- Let Spaces bind to a default Terminal Profile while terminal content records
  the profile used at creation time.
- Preserve current manifest compatibility and fallback behavior when profile
  references are missing, stale, or invalid.
- Add compact Settings and sidebar affordances consistent with the existing
  terminal-first macOS shell.

**Non-Goals:**

- Editing `/etc/sudoers`, configuring passwordless sudo, or managing Unix users.
- Replacing `connection_profile`, provider auth, `agent.toml`, or connection
  management.
- Creating a full Identity Profile that owns Git, SSH, cloud, agent, and
  terminal settings in one model.
- Migrating profile definitions through workspace manifests or project files.
- Changing the Ghostty terminal backend or adding another terminal runtime.
- Retrofitting already-running terminals when a Space binding or profile
  definition changes.

## Decisions

1. **Use a new Terminal Profile concept instead of extending connection profiles.**

   Terminal Profiles describe terminal startup identity. Connection profiles
   describe LLM/provider credentials and model selection. Keeping these models
   distinct prevents UI ambiguity and avoids pulling Unix account details into
   daemon/runtime connection semantics. A future Identity Profile can compose a
   Terminal Profile and a connection profile, but V1 keeps the terminal model
   narrow.

2. **Store profile definitions as machine-local, channel-scoped app support data.**

   The profile store lives under the active Alan macOS install channel's
   Application Support directory, for example `terminal-profiles.json` beside
   other channel-scoped shell state. This keeps `/Users/alan`, `sudo -iu alan`,
   icons, colors, and custom commands out of workspace manifests. Dev-channel
   testing can create profiles without silently mutating stable-channel user
   state.

3. **Persist only profile references in shell workspace state.**

   A Space record stores an optional `terminal_profile_id`. A terminal content
   payload stores the profile id resolved at content creation time. The
   workspace manifest never copies the profile definition. Missing references
   remain visible as missing references and fall back to the login shell instead
   of preventing app startup or deleting user workspace state.

4. **Keep launch modes structured where possible.**

   The profile kind is one of `login_shell`, `sudo_user`, `sudo_root`, or
   `custom_command`. Structured sudo modes generate argv directly, such as
   `/usr/bin/sudo -iu alan`, instead of asking users to handwrite shell strings.
   `custom_command` remains available as an advanced escape hatch and runs via
   `/bin/zsh -lc <command>`.

5. **Resolve profiles in shell model code before Ghostty surface creation.**

   `AlanShellBootProfile.forPane(...)` already produces command, cwd,
   environment, and Ghostty integration data. Terminal Profiles should extend
   that resolution path. The resulting Ghostty `surfaceConfig.command`,
   `working_directory`, and environment remain the terminal launch mechanism.

6. **Use Space binding as a default, not a retroactive mutation.**

   A Space profile affects future terminal creation. New tabs inherit the Space
   profile by default; new splits inherit the current pane's terminal profile by
   default; explicit creation requests can override either. Existing terminal
   content keeps the profile id it was created with. This avoids silently
   changing a running terminal's Unix user after a Space setting changes.

7. **Expose Terminal Profiles through calm local UI.**

   Settings gains a local Terminal Profiles section for creation and editing.
   Space menus or the Space header provide binding selection. Sidebar rows and
   pane title/status surfaces can show profile identity when useful, especially
   for root, custom command, or missing-profile states, but should not turn every
   row into a dense status dashboard.

## Risks / Trade-offs

- **[Risk] Profile terminology collides with provider connection profiles.** ->
  Use `Terminal Profile` in UI and `terminal_profile_id` in serialized fields.
  Avoid bare `profile` labels in this feature.
- **[Risk] Machine-local profile references break when moving manifests between
  Macs.** -> Preserve the id, show a missing-profile state, and fall back to
  login shell. Do not delete or rewrite the reference automatically.
- **[Risk] Custom commands expose sensitive local workflow details.** -> Keep
  custom commands in local profile storage, not manifests. Sidebar/control-plane
  summaries use id/title/kind by default; full command appears only in Settings
  or explicit diagnostics.
- **[Risk] Sudo prompts surprise users.** -> Alan does not hide sudo behavior.
  If sudo requires a password, the terminal shows the normal prompt. Settings
  can explain that passwordless sudo is an operator-managed system setting.
- **[Risk] Changing a profile definition changes restored terminals later.** ->
  This is intentional because terminal content stores a profile reference, not a
  snapshot. Users who need a frozen startup command can duplicate the profile
  before changing it.
- **[Risk] Channel-scoped profiles create duplicate setup for stable/dev.** ->
  Channel separation protects stable state during development. A later import or
  copy action can reduce duplicate setup after the feature stabilizes.

## Migration Plan

1. Add Terminal Profile value types and a channel-scoped local profile store
   with default login-shell fallback, schema validation, and corrupt-file
   quarantine.
2. Add optional `terminal_profile_id` fields to Space records and terminal
   content payloads while preserving old manifest decoding.
3. Extend shell state mutations, workspace materialization, and restore snapshot
   conversion to preserve profile references.
4. Extend boot-profile resolution to load Terminal Profiles and generate
   structured launch commands.
5. Add action/control-plane/App Intent fields for explicit terminal-profile
   overrides on Space, tab, and split creation.
6. Add Settings and sidebar UI affordances after the model and launch behavior
   are covered by focused tests.
7. Validate with model tests, terminal runtime tests, manifest tests, shell
   action/automation tests, and dev-channel relaunch smoke.

Rollback: because profile definitions are local and references are optional,
the feature can be disabled by ignoring `terminal_profile_id` fields and
falling back to login-shell startup. Existing manifests remain decodable.

## Open Questions

- Should stable-channel Alan eventually offer an explicit "Import profiles from
  Alan Dev" action after the feature ships?
- Should profile colors/icons be required for Space dock hints, or remain
  optional presentation metadata in V1?
- Should custom-command profiles require a one-time confirmation on first use,
  or is Settings-level advanced labeling sufficient?
