## 1. Remove Quick Terminal Product Surface

- [x] 1.1 Remove Quick Terminal shell action IDs, shortcut descriptors, titles,
  availability, execution handlers, and keyboard routing from Swift shell action
  registry paths.
- [x] 1.2 Remove Quick Terminal control-plane command DTO cases, command
  handlers, response paths, diagnostics, App Intents, and automation aliases.
- [x] 1.3 Remove Quick Terminal menu items, `Open in Space` affordances, Peak
  content UI, and Quick Terminal-specific terminal pane chrome.
- [x] 1.4 Remove Quick Terminal Peak presenter, panel/window types, focus
  budget, placement model, AppKit collection-behavior code, and shell-owner
  subscriptions.
- [x] 1.5 Remove Quick Terminal close scope and active-work close handling while
  preserving normal pane, tab, window, and app terminal close guards.

## 2. Clean Rust Shell Core And FFI Authority

- [x] 2.1 Remove quick-terminal action IDs, shortcut mapping, availability, and
  action execution effects from `crates/shell-core`.
- [x] 2.2 Remove quick-terminal model state, reducer operations, global IDs,
  promotion logic, close/hide/show/focus logic, and associated tests from
  `crates/shell-core`.
- [x] 2.3 Remove quick-terminal FFI request/response shapes and Swift FFI adapter
  mappings except for narrow legacy manifest decode tolerance.
- [x] 2.4 Ensure shell-core workspace state contains only real Spaces, Tabs,
  PaneSlots, ContentInstances, focus, action, and manifest fields that remain
  authoritative after Quick Terminal removal.

## 3. Implement Primary Window Summon

- [x] 3.1 Add a macOS app/window command for Primary Window Summon outside the
  shell action registry and Rust shell core.
- [x] 3.2 Reassign the former Quick Terminal global shortcut/menu surface to
  Primary Window Summon without preserving `quick_terminal` or
  `shell.quick_terminal.*` aliases.
- [x] 3.3 When the primary shell window is visible, summon the same window to the
  current active Space/display on a best-effort basis, activate the app, and bring
  the window forward.
- [x] 3.4 When the app is running but the primary shell window is closed, reopen
  or create the single primary shell window and focus it.
- [x] 3.5 Preserve selected shell Space, Tab, PaneSlot, split geometry, pane zoom
  state, and content runtime identity across summon.
- [x] 3.6 Focus selected terminal input after summon when the selected content is
  terminal; otherwise focus the window or selected view without switching to a
  terminal.

## 4. Manifest And Legacy Data Cleanup

- [x] 4.1 Update Swift manifest load/materialization to discard old visible or
  hidden `quick_terminal` records without creating quick-terminal shell state,
  terminal runtimes, tabs, panes, transcripts, or panels.
- [x] 4.2 Update Swift manifest writeback to omit `quick_terminal` and remove
  quick-terminal transcript snapshot preservation.
- [x] 4.3 Update Rust shell-core manifest decode/materialize/write tests so old
  `quick_terminal` records are tolerated only as discarded legacy data.
- [x] 4.4 Remove `quick_terminal` from new manifest fixtures, golden snapshots,
  DTO examples, and OpenSpec references that describe active behavior.

## 5. Verification

- [x] 5.1 Replace Quick Terminal focused tests in
  `clients/apple/scripts/test-shell-runtime-metadata.swift` and related scripts
  with Primary Window Summon, removed-surface, and legacy-discard coverage.
- [x] 5.2 Add or update focused checks proving menus, keybindings, action
  descriptors, FFI adapters, control commands, App Intents, and Rust shell-core
  surfaces no longer expose Quick Terminal actions or aliases.
- [x] 5.3 Add running-app smoke or documented manual verification for invoking
  Primary Window Summon from another macOS Space/display and for the activation
  fallback when movement cannot be guaranteed.
- [x] 5.4 Run `clients/apple/scripts/test-shell-runtime-metadata.sh` or the
  current focused Apple shell contract script that supersedes it.
- [x] 5.5 Run relevant Rust shell-core tests covering actions, reducer,
  manifest, and FFI contracts.
- [x] 5.6 Run the relevant macOS app build command or document any local blocker.
- [x] 5.7 Run `openspec validate replace-quick-terminal-with-primary-window-summon --type change --strict --json`.
- [x] 5.8 Run `openspec validate --all --strict --json`.
- [x] 5.9 Run `git diff --check`.

## 6. Archive Readiness

- [x] 6.1 Review active code and docs for lingering user-facing Quick Terminal,
  Peak, `quick_terminal`, or `shell.quick_terminal.*` references and keep only
  historical archived OpenSpec material or explicit legacy-decode comments.
- [x] 6.2 Sync accepted delta requirements into `openspec/specs/` before
  archiving after implementation merges.
- [ ] 6.3 Archive the completed OpenSpec change after implementation and PR
  merge.
