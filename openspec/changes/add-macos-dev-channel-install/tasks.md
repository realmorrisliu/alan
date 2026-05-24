## 1. Channel Model And Path Resolution

- [x] 1.1 Add a typed install-channel descriptor for `stable` and `dev` identities, including app name, bundle id, CLI/TUI names, alan home, global skill source, daemon defaults, and shell-control namespace.
- [x] 1.2 Thread channel selection into CLI, daemon, TUI, and macOS app startup before any config, auth, daemon, or runtime path is resolved.
- [x] 1.3 Update runtime alan-home path resolution so stable uses `~/.alan` and dev uses `~/.alan-dev` for host config, registry, agents, models, connections, credentials, sessions, memory, managed auth, and caches.
- [x] 1.4 Update agent-root layout resolution and writes so global roots are channel-scoped while workspace roots remain unchanged.
- [x] 1.5 Update global public skill discovery and install/update flows so stable uses `~/.agents/skills` and dev uses `~/.agents-dev/skills`.

## 2. macOS Packaging And Install Scripts

- [x] 2.1 Make app assembly channel-aware without changing stable `Alan.app` output.
- [x] 2.2 Add dev app metadata/build overrides for `Alan Dev.app` and `app.alanworks.macos.dev`.
- [x] 2.3 Embed channel-appropriate CLI/TUI binaries as `alan`/`alan-tui` for stable and `alan-dev`/`alan-dev-tui` for dev.
- [x] 2.4 Add `just install-dev` and dev install script behavior that installs `Alan Dev.app` plus `alan-dev`/`alan-dev-tui` without touching stable artifacts.
- [x] 2.5 Add `just uninstall-dev` and dev uninstall behavior that removes only dev-owned app and command links while preserving `~/.alan-dev` data.
- [x] 2.6 Keep public release, Homebrew cask, and Sparkle publication paths stable-only.

## 3. Daemon, TUI, And App Runtime Isolation

- [x] 3.1 Add distinct dev daemon/client defaults so `alan-dev` and `alan-dev-tui` do not connect to the stable daemon implicitly.
- [x] 3.2 Ensure missing dev connection/auth config produces onboarding or configuration-required state instead of falling back to stable credentials.
- [x] 3.3 Scope macOS singleton locks, support paths, log subsystems, capture-helper defaults, and diagnostics by channel.
- [x] 3.4 Scope shell-control socket and binding paths by channel so stable and dev commands cannot read each other's control files.
- [x] 3.5 Verify stable and dev apps can run side by side without activating, terminating, or hijacking each other's singleton state.

## 4. Workspace Generated State

- [x] 4.1 Add a channel namespace for generated workspace runtime state such as sessions, memory, cache, shell restore, and runtime metadata.
- [x] 4.2 Preserve stable compatibility with existing legacy generated state while ensuring dev never writes to stable legacy generated paths.
- [x] 4.3 Update repository ignore rules and docs so channel-scoped generated state is ignored and authored `.alan/agents/` / `.agents/skills/` content remains trackable.

## 5. Verification And Guardrails

- [x] 5.1 Add focused tests for channel descriptor values and channel-aware path resolution.
- [x] 5.2 Add packaging/install contract checks for `Alan Dev.app`, `alan-dev`, `alan-dev-tui`, and stable artifact preservation.
- [x] 5.3 Add guardrails or allowlists for dev-channel brand strings without weakening stable brand validation.
- [x] 5.4 Add tests for channel-scoped connection stores, managed auth stores, global public skills, daemon defaults, and shell-control paths.
- [x] 5.5 Run a documented side-by-side smoke with stable Alan installed while Alan Dev launches and writes only dev-channel state.
- [x] 5.6 Run `openspec validate add-macos-dev-channel-install --strict` and `openspec validate --all --strict`.

## 6. Review And Archive Readiness

- [x] 6.1 Keep implementation commits scoped to this change and avoid mixing with unrelated macOS shell work.
- [x] 6.2 Before PR review, confirm public stable install, Homebrew, and Sparkle contracts remain unchanged.
- [ ] 6.3 After merge, archive the change and verify the delta specs are folded into long-lived `openspec/specs/` owners.
