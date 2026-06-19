# Apple Client Architecture Maintainability

This document records the current Apple client source ownership baseline and the
target layout for behavior-preserving refactor slices. It is intentionally about
maintainability, not product behavior.

## Current Inventory

The Apple client is being migrated out of the original flat
`clients/apple/alan-macos` source directory. Files listed with owner folders are
already split into the target layout and remain members of the `alan-macos`
Xcode target.

| File | Lines | Platform / bridge imports | Primary responsibility today | Target owner |
| --- | ---: | --- | --- | --- |
| `AlanApp.swift` | 34 | SwiftUI; macOS gates | Thin app entry and scene composition | `App/` |
| `App/AlanMacAppDelegate.swift` | 13 | AppKit; macOS gates | Reopen handling for the primary alan window | `App/` |
| `App/AlanMacAppStartup.swift` | 19 | Darwin; macOS gates | Duplicate-instance startup and singleton guard handling | `App/` |
| `App/AlanMacPrimaryShellOwner.swift` | 21 | Foundation, SwiftUI; macOS gates | Primary `window_main` shell owner creation | `App/` |
| `App/AlanMacPrimaryWindowPresenter.swift` | 20 | AppKit; macOS gates | Primary alan window focusing and activation | `App/` |
| `App/AlanMacShellCommands.swift` | 91 | SwiftUI; macOS gates | App menu and keyboard command definitions routed through shell workspace commands | `App/` |
| `AlanAppSingletonGuard.swift` | 141 | Foundation, AppKit, Darwin; macOS gates | OS-backed duplicate-instance guard | `App/` or `Support/Windowing/` |
| `Support/ShellDesignTokens.swift` | 200 | AppKit, SwiftUI; macOS gates | Shell palette, corner radii, and native material wrapper | `Support/` |
| `Support/ShellWindowPlacement.swift` | 205 | AppKit, SwiftUI; macOS gates | Hidden-titlebar placement, min-size, traffic-light metrics, and primary window activation | `Support/` |
| `Support/ShellVoiceCommandController.swift` | 63 | AppKit, SwiftUI; macOS gates | Narrow speech-recognizer bridge for command palette voice actions | `Support/` |
| `MacShellRootView.swift` | 63 | SwiftUI; macOS gates | Thin primary shell composition root | `Views/Shell/` |
| `Views/Shell/ShellSidebarView.swift` | 538 | SwiftUI; macOS gates | Primary shell sidebar, tab rows, space dock, and sidebar state | `Views/Shell/` |
| `Views/Shell/ShellWorkspaceView.swift` | 46 | SwiftUI; macOS gates | Shell workspace composition and space keyboard shortcuts | `Views/Shell/` |
| `Views/Shell/ShellCommandTabView.swift` | 621 | SwiftUI; macOS gates | Command palette search, routing, attention, and action presentation | `Views/Shell/` |
| `TerminalPaneView.swift` | 3008 | Foundation, SwiftUI; macOS gates | Split-tree, pane leaf rendering, title bars, and restored transcript presentation | `Views/Shell/Terminal/` |
| `TerminalHostView.swift` | 1911 | AppKit, Carbon, QuartzCore, GhosttyKit; macOS gates | AppKit terminal host bridge, focus, overlay composition, runtime attachment, and collaborator wiring | `Views/Shell/Terminal/` plus terminal collaborators |
| `GhosttyLiveHost.swift` | 1227 | Foundation, AppKit, GhosttyKit, OSLog, QuartzCore; macOS/Ghostty gates | Ghostty canvas bridge and wakeup/occlusion integration | `Services/Terminal/` or `Support/TerminalBridge/` |
| `TerminalHostRuntime.swift` | 1406 | CoreGraphics, Foundation; macOS gates | Terminal launch resolution, boot profiles, runtime protocols, and fallback runtime state | `Services/Terminal/` |
| `Services/Terminal/TerminalHostRuntimeReporter.swift` | 47 | Foundation; macOS gates | Runtime snapshot deduplication and main-queue publication for terminal host updates | `Services/Terminal/` |
| `Services/Terminal/TerminalHostWindowObserver.swift` | 55 | AppKit; macOS gates | Terminal host window key, screen, and occlusion notification ownership | `Services/Terminal/` |
| `TerminalRuntimeRegistry.swift` | 598 | SwiftUI; macOS gates | Pane/content-keyed terminal host/runtime registry | `Services/Terminal/` |
| `TerminalRuntimeService.swift` | 1815 | Foundation, AppKit, GhosttyKit; macOS/Ghostty gates | Window-scoped terminal runtime service, lifecycle ownership, and Ghostty bootstrap | `Services/Terminal/` |
| `TerminalSurfaceController.swift` | 1735 | Foundation, AppKit, GhosttyKit; macOS/Ghostty gates | Terminal input, pointer, scrollback, search, semantic commands, and surface adapters | `Services/Terminal/` |
| `Models/Shell/ShellValueTypes.swift` | 2178 | Foundation | Shell command enums, launch targets, process bindings, Terminal Profile DTOs, and managed-account platform/effect shapes | `Models/Shell/` plus future shell service collaborators |
| `Models/Shell/ShellSnapshots.swift` | 2093 | CoreGraphics, Foundation | Shell panes, tabs, spaces, split tree, state snapshots, snapshot query helpers, and portable projection DTOs | `Models/Shell/` |
| `Models/Shell/ShellStateRuntimeSupport.swift` | 348 | Foundation | Narrow app-target shell bootstrap defaults, mutation result/error types, Terminal Profile inheritance queries, platform activity acknowledgement/projection, and inactive temporary-tab query support | `Models/Shell/` |
| `ShellModel.swift` | 1227 | Foundation | Shell title, label, sidebar, and status presentation helpers | `Models/Shell/` or `Support/ShellPresentation/` |
| `ShellHostController.swift` | 4424 | Foundation, AppKit, SwiftUI; macOS gates | Observable shell controller, runtime update intake, command routing, and shell state mutation coordination delegated to services | `Controllers/Shell/` plus service collaborators |
| `Controllers/Shell/ShellHostControlCommandHandling.swift` | 1805 | Foundation; macOS gates | Shell control-plane command response handling, host routing, terminal delivery, and list helpers | `Controllers/Shell/` plus future control response collaborators |
| `Services/Shell/ShellControlFilePoller.swift` | 182 | Foundation; macOS gates | File-backed command/result polling and alan binding-file projection | `Services/Shell/` |
| `Services/Shell/ShellDiagnostics.swift` | 16 | Foundation; macOS gates | Shell service diagnostic routing | `Services/Shell/` |
| `Services/Shell/ShellEventStore.swift` | 677 | Foundation; macOS gates | Shell event buffering, diffing, `events.read`, and jsonl persistence | `Services/Shell/` |
| `Services/Shell/ShellLocalCommandExecutor.swift` | 922 | Foundation; macOS gates | Local shell control command execution against shell state and terminal delivery effects | `Services/Shell/` |
| `Services/Shell/ShellPaneProjectionService.swift` | 302 | Foundation; macOS gates | Pane boot context, runtime metadata, viewport, attention, and alan binding projection | `Services/Shell/` |
| `Services/Shell/ShellPublishedStateMerger.swift` | 193 | Foundation; macOS gates | Merge published shell state with authoritative runtime metadata | `Services/Shell/` |
| `Services/Shell/ShellSocketServer.swift` | 415 | Foundation, Darwin; macOS gates | Bounded local socket transport, request parsing, and client response handling | `Services/Shell/` |
| `Services/Shell/ShellStatePersistenceStore.swift` | 221 | Foundation; macOS gates | Shell state save/restore, persistence URL selection, and restored window context lookup | `Services/Shell/` |
| `ShellControlPlane.swift` | 463 | Foundation; macOS gates | Shell control-plane orchestration across socket, file polling, state publishing, pane support directories, event store, and diagnostics | `Services/Shell/` |
| `Models/API/DaemonAPIModels.swift` | 529 | Foundation | Daemon API response DTOs, operation payloads, JSON values, and API error type | `Models/API/` |
| `Models/Console/ConsoleModels.swift` | 148 | Foundation | Console chat messages, timeline entries, structured questions, and pending-yield value state | `Models/Console/` |
| `Services/Daemon/AlanAPIClient.swift` | 292 | Foundation | Daemon HTTP client, request construction, endpoint routing, and response validation | `Services/Daemon/` |
| `Services/Daemon/ConsoleEventReducer.swift` | 195 | Foundation | Console event page reader and event-to-message/timeline/pending-yield projection reducer | `Services/Daemon/` |
| `Controllers/Console/AlanConsoleViewModel.swift` | 609 | Foundation, SwiftUI | Legacy/mobile console observable state, action coordination, and event pump ownership | `Controllers/Console/` |
| `Views/Console/ContentView.swift` | 808 | SwiftUI | Legacy/mobile console UI composition | `Views/Console/` |
| `Views/Console/ConsoleSupportViews.swift` | 262 | SwiftUI | Console theme tokens, button styles, message bubbles, and timeline rows | `Views/Console/` |
| `Support/ConsoleAdaptiveColor.swift` | 33 | SwiftUI, AppKit; iOS/macOS gates | Platform-adaptive console color bridge | `Support/` |

## Target Layout

The accepted target under `clients/apple/alan-macos` is:

- `App/`: `AlanApp`, app delegate, duplicate-instance startup, primary
  shell owner creation, app commands, and primary window coordination.
- `Views/Shell/`: the default macOS shell composition, sidebar, workspace,
  command palette, pane title/search UI, and shell-specific SwiftUI components.
- `Views/Console/`: mobile or legacy remote-control console screens and local
  console view support that are not the primary macOS shell path.
- `Models/`: API DTOs, shell snapshots, shell IDs, enums, value types, and
  current-format decoding.
- `Controllers/`: observable app and shell controllers that own UI state and
  delegate IO or domain work to services. This folder currently records the
  target owner while `ShellHostController.swift` remains tracked migration debt.
- `Services/`: daemon API clients, event readers/reducers, terminal runtime
  services, Ghostty bootstrap, shell projection services, shell control plane,
  socket server, persistence, and other process or IO code.
- `Support/`: design tokens, formatting helpers, window placement, AppKit
  adapters, and small utilities.

## Shell Core Boundary

Reusable shell workspace semantics are migrating to the platform-neutral
`alan-shell-core` Rust crate. The crate owns the durable domain contract for
Spaces, Tabs, PaneSlots, ContentInstances, split trees, reducers, manifest
semantics, shared actions, control-command outcomes, Terminal Profile launch
intent resolution, settings summaries, coarse request/response envelopes, Rust
contract tests, and FFI-backed Swift adapter validation.

The Apple client remains the platform adapter. It owns SwiftUI/AppKit
presentation, windowing, menu and keyboard rendering, terminal runtime
attachment, Ghostty/AppKit bridge objects, file reads and writes, clipboard,
file pickers, Sparkle/update UI, diagnostics presentation, App Intents, and
privileged macOS account effects.

Swift shell model and controller files may keep compatibility wrappers while a
domain module is being migrated, but once the corresponding shell-core module
has Rust contract tests and adapter tests, new reusable domain behavior belongs in Rust rather than a second Swift implementation.

The first Swift integration facade uses a hand-written synchronous C ABI over
versioned JSON byte envelopes, implemented in `alan-shell-core-ffi`. This keeps
the pure Rust core binding-agnostic and avoids generated Swift/header/modulemap
churn until a later slice explicitly introduces a binding generator such as
UniFFI. The Swift adapter owns request encoding, response decoding, stable error
mapping, ABI-version checks, and schema-version mismatch handling.
The initial facade dispatch covers manifest materialization, reducer apply,
control-command handling, action registry lookup/execution, Terminal Profile
validation/editing/launch-intent resolution, and settings summary rows; Swift
production call paths should migrate through that adapter module by module after
each contract gate.
The macOS target now builds and bundles `libalan_shell_core_ffi.dylib`; the
workspace manifest store delegates default manifest creation and legacy manifest
migration to Rust, while workspace-manifest startup delegates TTL pruning and
materialization to Rust before projecting the portable state back into the
current macOS runtime snapshot shape. Swift keeps manifest file IO, corrupt-file
quarantine, and compatibility fallbacks at the platform adapter boundary.
Pane focus, adjacent-focus, terminal Space create, terminal Space delete, Space
Terminal Profile metadata, Space presentation icon metadata, terminal tab open,
terminal tab duplicate, terminal pane split, tab close, pane close, pin/unpin, reorder,
move-to-space, rename, split resize/equalize, within-tab pane move,
inactive-tab cleanup, and attention production reducer calls now use the
Rust-backed adapter for workspace panes.
Terminal-creating reducer requests carry platform-reserved pane IDs from the
macOS terminal runtime registry so Rust ID allocation does not collide with live
runtime owners. Detached quick-terminal focus remains a macOS runtime exception
because it is not mounted in Rust workspace pane slots, and command-failure
activity acknowledgement remains a Swift adapter-layer pass after Rust focus
until the portable reducer owns that semantic explicitly. Runtime-bearing
reducer branches that still lack terminal adapter wiring remain in Swift until
their runtime intents are explicitly connected. Shared shell action title,
availability, shortcut, keyboard lookup, and effect resolution now come from the
Rust action registry before Swift executes platform presentation or terminal
effects. Socket-local reusable workspace control commands, including state/list,
Space create, tab open/close/reorder/pin/move, pane split/close/lift/move/
focus/zoom, terminal send, and attention updates, now route through Rust
`control.handle`; Swift loads host-local Terminal Profile files and supplies
platform context, while shell-core decides whether the global default Terminal
Profile should be captured for new panes. Control handlers that still own
command-specific response details route
their reusable reducer step, including pane resize, equalize, spatial focus, and
within-tab move, through the Rust reducer adapter before recording host events.
Host-required, diagnostic, render-metric, quick-terminal, and unsupported
terminal-delivery commands stay in Swift. Settings surface row summaries for
Terminal Profiles, capabilities, and local diagnostics are
adapter-first Rust calls; Swift still owns section composition, navigation,
host file discovery, and managed-account platform effects.
Terminal Profile launch intent resolution in `TerminalHostRuntime` now asks Rust
for strategy, argv, profile resolution state, working-directory hints, and
environment projection; Swift still supplies executable availability, loads the
profile store, chooses the final pane working directory, and performs macOS
process/runtime attachment.

## Shell Core Authority Audit

The `make-shell-core-authoritative` cleanup treats the current shell-core
integration as real product code rather than a parity experiment. Remaining
Swift around `ShellCoreFFIAdapter` is classified as follows:

- Domain duplicate removed from runtime startup: workspace-manifest startup now
  requires Rust default/prune/materialize authority and no longer falls back to
  Swift `ShellContentWorkspaceManifest.defaultManifest`,
  `ShellContentWorkspaceManifest.pruningExpiredTabs`, or
  `ShellWorkspaceMaterializer.materialize`. The old Swift default, prune,
  materialize, and legacy migration parity algorithms were removed; default app
  builds keep only manifest DTOs, decode/selection repair, transcript cleanup,
  projection helpers, and manifest file IO.
- Domain duplicate removed from reducer and control paths: workspace reducer
  mutations and reusable local control commands now use shell-core failures,
  stable core errors, or explicit host-only command paths instead of a second
  Swift validation/mutation switch. The local executor keeps macOS-only
  diagnostics, terminal runtime delivery, and quick-terminal host behavior
  separate from portable shell-domain commands.
- Domain duplicate removed from action, Terminal Profile, and settings paths:
  shared action titles, availability, shortcuts, keyboard mapping, and effects
  call shell-core and fail closed on core errors. Explicit Terminal Profile
  launch intent resolution no longer falls back to Swift profile resolution
  after core failure; it returns an explicit fail-closed launch resolution. The
  no-explicit-profile default terminal path keeps the accepted host-availability
  fallback to native login-shell resolution when shell-core cannot load, and
  runtime metadata tests require explicit profiles to keep failing closed in that
  same failure mode. Terminal Profile validation, editor definition construction,
  document upsert,
  profile-store load/save validation, and global default capture policy now
  route through `ShellCoreFFIAdapter` from
  `Services/Shell/TerminalProfileStore.swift`; managed-account Terminal
  Profile handoff also uses that Rust-backed document editor instead of
  mutating profile documents locally. Managed-account request validation and
  provisioning-plan decisions now call `managed_terminal_account.validate_request`
  and `managed_terminal_account.plan` through
  `Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift`, failing
  closed when shell-core is unavailable. `ShellValueTypes.swift` keeps the DTOs,
  rollback plan, sudoers/platform effect helpers, and error/result shapes rather
  than a second Swift implementation of Rust-owned validate/plan semantics.
  Reusable settings rows now come from shell-core or collapse to unavailable
  rows.
- Parity fixture debt removed: migration fixture corpora, fixture exporters,
  Swift action/manifest parity support, and Swift reducer/tree
  parity support are gone. Script tests that still need constructed shell
  states use `clients/apple/scripts/support/ShellCoreFFITestStateBuilder.swift`,
  which prepares state through `ShellCoreFFIAdapter.applyReducer`, while Rust
  `crates/shell-core/tests/*_contract.rs` owns the action, manifest, reducer,
  and split-tree behavior contracts. Runtime code is guarded from using
  `ShellActionRegistry.standard`.
- Adapter projection to keep narrow: `ShellCoreFFIAdapter` is now a facade with
  sibling loader, envelope, materialization, and operation-family owners,
  including separate Terminal Profile and managed terminal account adapters.
  The materialization owner projects core state into current Swift runtime DTOs
  and preserves platform-only pane fields such as runtime metadata, renderer
  state, display identity, and terminal activity while avoiding independent
  domain decisions.
- Host startup/persistence extracted from the observable controller:
  `ShellWorkspaceManifestStartupCoordinator` owns workspace manifest load,
  Rust-core pruning, Rust-core materialization, and startup diagnostics, while
  `ShellWorkspacePersistenceCoordinator` owns manifest writer construction,
  manifest save debounce, shell-state file writes, and control-plane flush
  cadence. `ShellHostController` now supplies UI/runtime projection closures
  instead of owning those persistence rules directly.
- Host action/reducer/metadata routing extracted from the observable
  controller: `ShellActionCoordinator` is the only Swift owner that calls the
  Rust action registry FFI, `ShellReducerCommandCoordinator` is the only Swift
  owner that calls the Rust reducer FFI, and
  `ShellPlatformMetadataPreserver` owns post-adoption preservation of live
  macOS-only pane context and runtime metadata.
- Platform recovery/effect to keep in Swift: manifest file IO, corrupt-file
  quarantine, diagnostics, Ghostty/runtime recovery, terminal input delivery,
  pasteboard/keyboard handling, windowing, SwiftUI/AppKit presentation, and
  privileged macOS account effects.
- Runtime exceptions to track: detached quick-terminal focus and command-failure
  activity acknowledgement remain Swift adapter-layer behavior until the core
  has explicit portable semantics for those cases.

Swift workspace-manifest and runtime-metadata tests now call the shell-core
adapter for default manifest creation, pruning, materialization, and legacy
terminal manifest migration, while keeping Swift assertions focused on corrupt
file recovery, persistence, app projection, and platform runtime metadata.
Action, control, settings, and Terminal Profile focused tests likewise exercise
the shell-core adapter or Rust contract tests for portable domain behavior.

## Apply Sequence Notes

- Start with report-mode checks and pure model/support moves.
- Keep behavior changes out of mechanical move commits.
- `polish-macos-search-remove-inspector` and
  `normalize-macos-shell-corner-radii` were archived before the shell-root
  split. Keep future UI behavior work, such as `add-macos-pane-title-bars`,
  rebased on top of the current shell component files instead of burying
  behavior changes inside architecture-only slices.
- Split terminal host and control-plane collaborators only with focused runtime
  or IPC script checks in the same slice.

## Validation

Run the architecture report directly:

```bash
bash clients/apple/scripts/check-architecture-maintainability.sh
```

The default mode reports known migration debt and fails only on narrow
regressions such as new root-level Swift files or Xcode project membership drift.
Use `--strict` when intentionally tightening the architecture gate.

## Implementation Evidence

The architecture-maintainability implementation was completed as behavior-
preserving PR slices. The final validation pass before syncing this spec ran:

- `bash clients/apple/scripts/test-terminal-runtime-service.sh`
- `bash clients/apple/scripts/test-terminal-surface-controller.sh`
- `bash clients/apple/scripts/test-shell-runtime-metadata.sh`
- `bash clients/apple/scripts/check-shell-contracts.sh`
- `bash clients/apple/scripts/check-architecture-maintainability.sh`
- `git diff --check`
- `openspec validate improve-macos-app-architecture-maintainability --type change --strict --json`
- `openspec validate --all --strict --json`
- `xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -destination generic/platform=macOS -derivedDataPath target/xcode-derived build`

The macOS build succeeded. Local Xcode continued to print the existing
CoreSimulator version warning while building for `generic/platform=macOS`; simulator
device support was not required for this validation.

## Remaining Architecture Debt

`check-architecture-maintainability.sh` currently completes in report mode with
13 known large-file / bridge-boundary warnings after the post-core cleanup
slice moved Swift reducer parity support out of the app target. These warnings
are telemetry for the cleanup, not the cleanup
definition. The real debt is any Swift production source that still carries a
Rust-owned shell-domain implementation, fixture, or fallback after shell-core
has become authoritative.

### Post-Core Slimming Baseline

Baseline captured on 2026-06-18 from the
`feat/introduce-cross-platform-shell-core` worktree after the
`make-shell-core-authoritative` implementation reached PR review. PR #560 was
still open and review-blocked, so the slimming change uses the current branch's
final shell-core authority boundary as its implementation baseline until the
authority PR is merged and archived.

The report-mode warning classes before the first cleanup slice were:

- 16 large Swift files over the 1,200-line report threshold.
- 1 bridge-boundary warning for `ShellHostController.swift` importing AppKit
  while still outside a narrow bridge owner.

After moving manifest/action parity support out of production Apple sources,
splitting the shell-core FFI facade, moving Terminal Profile validation/editor/
store behavior behind the Rust-backed service owner, moving fixture-only managed
account fake execution/profile handoff support into script support, routing
managed-account validation, provisioning planning, and settings row projection
through shell-core FFI, moving Swift reducer/tree mutation parity support out of
the app target, and
extracting manifest startup, persistence, action, reducer, and platform metadata
coordinators from `ShellHostController.swift`, the warning classes are:

- 12 large Swift files over the report threshold.
- 1 bridge-boundary warning for `ShellHostController.swift` importing AppKit
  while still outside a narrow bridge owner.

Rust-owned Swift legacy cleanup targets at this baseline:

| File | Lines | Cleanup target |
| --- | ---: | --- |
| `Models/Shell/ShellWorkspaceManifest.swift` | 513 | Cleaned: Swift manifest default/prune/materialize/migration parity implementations and fixture exporters are removed. Production keeps DTOs, repair, transcript cleanup, projection helpers, and file IO while Rust manifest contract tests own portable behavior. |
| `Models/Shell/ShellActionRegistry.swift` | 248 | Cleaned: the Swift standard action registry table and resolver fixture are removed. Production keeps action IDs, targets, effects, keyboard action values, and terminal command target resolution while Rust action tests plus FFI adapter tests own registry behavior. |
| `Models/Shell/ShellStateRuntimeSupport.swift` | 348 | Cleaned: production no longer compiles `ShellStateMutations.swift` or `ShellTreeMutations.swift`, and the script-side duplicate Swift reducer/tree implementations are removed. The app target keeps only bootstrap defaults, mutation result/error shapes, Terminal Profile inheritance queries, platform activity acknowledgement/projection, and inactive temporary-tab query support. Script state construction that still needs mutations uses the FFI-backed `ShellCoreFFITestStateBuilder.swift`. |
| `Models/Shell/ShellValueTypes.swift` | 2,178 | Partially cleaned: Terminal Profile validator/editor/store behavior moved to `Services/Shell/TerminalProfileStore.swift`, where validation, editor definition construction, document upsert, and global default capture policy call shell-core FFI and fail closed on core errors. Fixture-only managed-account fake execution/profile handoff support now lives in `clients/apple/scripts/support/ManagedTerminalAccountTestSupport.swift`. Managed-account request validation and provisioning planning now call shell-core FFI and fail closed on core errors. Production value types still keep Terminal Profile DTOs, error/result shapes, managed-account platform models, local discovery/readiness, sudoers file validation/projection, rollback planning, and authorized executor helpers until later platform-effect or FFI adapter slices. |
| `Models/Shell/ShellSettingsSurfaceModel.swift` | 1,195 | Cleaned below the current report threshold: managed terminal account settings row icon/status/detail projection now calls `settings.managed_terminal_account_rows` through `Services/Shell/ShellCoreFFISettingsAdapter.swift`; production keeps settings navigation/grouping DTOs, remote/local host summary collection, workspace context discovery, and fail-closed unavailable rows. |
| `Services/Shell/ShellCoreFFIAdapter.swift` | 12 | Cleaned as adapter facade: loader, envelope, materialization, manifest, reducer, control, action, settings, Terminal Profile, and managed terminal account operation owners now live in sibling `ShellCoreFFI*` files; no Swift domain fallback was added. |
| `ShellHostController.swift` | 4,424 | Partially cleaned: workspace-manifest startup, Rust-core pruning/materialization, manifest writer construction, debounce scheduling, shell-state persistence, control-plane flush cadence, action dispatch/effect routing, reducer invocation including quick-terminal focus, and platform metadata preservation now live in named `Services/Shell/*Coordinator` or `*Preserver` owners. Remaining cleanup target: observable UI/runtime orchestration and any control response adoption still better owned outside the host controller. |
| `Controllers/Shell/ShellHostControlCommandHandling.swift` | 1,805 | Partially cleaned: reusable reducer invocations now route through `ShellReducerCommandCoordinator`. The remaining bulk is platform-side control-plane request/response projection, terminal delivery, diagnostics, quick-terminal routing, and list helpers rather than a second Rust-owned mutation implementation. Remaining cleanup target: split control response projection and host routing into narrower collaborators after the controller split. |

The first implementation batch targeted the production Swift legacy surface, not
a line-count threshold. Manifest parity helpers and the Swift standard action
registry plus reducer/tree mutation parity support moved out of normal
app-target sources into script support. Direct inspection of
`ShellHostControlCommandHandling.swift` shows no direct shell-core FFI owner or
Swift reducer fallback remains there; its unresolved size is platform glue that
should be split separately from Rust-domain authority cleanup. The
reduced architecture warning count is a byproduct; success is defined by the
absence of production-compiled Rust-owned Swift implementations and by
`check-shell-contracts.sh` continuing to reject fallback paths.

The current architecture gate remains non-blocking for future documented
warnings while failing narrower regressions such as new root-level Swift files,
project membership drift, reintroduced control-plane ownership in the wrong
file, direct reducer FFI calls outside `ShellReducerCommandCoordinator`, or
direct shell-core FFI calls outside the documented operation owner allowlist.
Direct `ShellCoreFFIAdapter` construction is likewise restricted to the loader
owner, and raw `alan_shell_core_ffi_*` symbol access must stay in that same
loader owner, so production code cannot bypass the shared adapter boundary.
The `macos-app-architecture-maintainability` spec requires this debt record to
stay current whenever warnings are introduced, broadened, or resolved.
