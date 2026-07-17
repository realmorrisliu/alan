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
| `Views/Shell/ShellSidebarView.swift` | 882 | SwiftUI, UniformTypeIdentifiers; macOS gates | Sidebar composition, Space paging, activity freshness, and tab-list orchestration | `Views/Shell/` |
| `Views/Shell/ShellSidebarTabDrop.swift` | 158 | SwiftUI, UniformTypeIdentifiers; macOS gates | Tab-list offsets, drop targeting, and insertion feedback | `Views/Shell/` |
| `Views/Shell/ShellSidebarSpaceSlider.swift` | 739 | SwiftUI; macOS gates | Space selection, scrub and wheel input, profile menus, and attention presentation | `Views/Shell/` |
| `Views/Shell/ShellSidebarTabRow.swift` | 447 | SwiftUI; macOS gates | Tab row chrome, labels, activity details, close controls, and empty actions | `Views/Shell/` |
| `Views/Shell/ShellSidebarActivityProgressRail.swift` | 45 | SwiftUI; macOS gates | Compact sidebar activity progress presentation | `Views/Shell/` |
| `Views/Shell/ShellPaneTopologyIndicator.swift` | 285 | SwiftUI; macOS gates | Pane topology rendering and direct split-pane focus actions | `Views/Shell/` |
| `Views/Shell/ShellWorkspaceView.swift` | 46 | SwiftUI; macOS gates | Shell workspace composition and space keyboard shortcuts | `Views/Shell/` |
| `Views/Shell/ShellCommandTabView.swift` | 621 | SwiftUI; macOS gates | Command palette search, routing, attention, and action presentation | `Views/Shell/` |
| `TerminalPaneView.swift` | 229 | Foundation, SwiftUI; macOS gates | Workspace canvas composition, preview selection, and empty-state panel presentation | `Views/Shell/Terminal/` |
| `Views/Shell/Settings/ShellSettingsContentView.swift` | 496 | Foundation, OSLog, SwiftUI; macOS gates | Settings composition, state refresh, managed-user actions, and diagnostics export | `Views/Shell/Settings/` |
| `Views/Shell/Settings/ShellSettingsNavigationView.swift` | 190 | SwiftUI; macOS gates | Settings navigation rail, group selection, and pane backgrounds | `Views/Shell/Settings/` |
| `Views/Shell/Settings/ShellSettingsComponents.swift` | 644 | SwiftUI; macOS gates | Settings rows, action accessories, managed-user sheets, metrics, and typography | `Views/Shell/Settings/` |
| `Views/Shell/Terminal/ShellPaneTreeLayoutView.swift` | 363 | Foundation, SwiftUI; macOS gates | Pane-tree dispatch, split layout, resize gestures, and leaf action wiring | `Views/Shell/Terminal/` |
| `Views/Shell/Terminal/ShellTerminalLeafView.swift` | 171 | Foundation, SwiftUI; macOS gates | Terminal leaf composition, restored transcript presentation, and pane-scoped find overlay wiring | `Views/Shell/Terminal/` |
| `Views/Shell/Terminal/ShellPaneTitleBarViews.swift` | 585 | Foundation, SwiftUI; macOS gates | Terminal and bounded-content title bars, responsive accessories, and activity freshness | `Views/Shell/Terminal/` |
| `Views/Shell/Terminal/ShellTerminalOverlayViews.swift` | 128 | SwiftUI; macOS gates | Pane-scoped Find interaction and passive inactive-pane dimming | `Views/Shell/Terminal/` |
| `Views/Shell/Content/ShellBoundedContentViews.swift` | 647 | Foundation, SwiftUI; macOS gates | Bounded Agent and Markdown content renderers plus unavailable-content presentation | `Views/Shell/Content/` |
| `Views/Shell/Terminal/TerminalHostView.swift` | 574 | AppKit, QuartzCore, GhosttyKit; macOS gates | AppKit terminal host lifecycle, runtime attachment, overlay composition, and collaborator wiring | `Views/Shell/Terminal/` |
| `GhosttyLiveHost.swift` | 1179 | Foundation, AppKit, GhosttyKit, OSLog, QuartzCore; macOS/Ghostty gates | Ghostty app/surface lifecycle, callbacks, and renderer coordination | `Services/Terminal/` |
| `Services/Terminal/GhosttyPlatformAdapters.swift` | 104 | Foundation, AppKit, GhosttyKit; macOS/Ghostty gates | Ghostty key-code, clipboard, app-focus, canvas, and display adapters | `Services/Terminal/` |
| `Services/Terminal/TerminalHostFocusAndPointerInput.swift` | 427 | AppKit, GhosttyKit; macOS/Ghostty gates | First-responder activation, pointer translation, mouse delivery, pressure, and scroll routing | `Services/Terminal/` |
| `Services/Terminal/TerminalHostInputTracing.swift` | 82 | AppKit; macOS gates | Host input trace projection, timing, and AppKit responder/view diagnostics | `Services/Terminal/` |
| `Services/Terminal/TerminalHostKeyboardInput.swift` | 637 | AppKit, Carbon, GhosttyKit; macOS/Ghostty gates | Physical keyboard translation, shell shortcut routing, Ghostty key delivery, and terminal command adaptation | `Services/Terminal/` |
| `Services/Terminal/TerminalHostTextInput.swift` | 173 | AppKit, GhosttyKit; macOS/Ghostty gates | `NSTextInputClient`, IME composition, selection, search, and semantic command adaptation | `Services/Terminal/` |
| `Services/Terminal/TerminalInputTrace.swift` | 134 | Foundation; macOS gates | Opt-in terminal input trace configuration, refresh, and diagnostic file sink | `Services/Terminal/` |
| `Services/Terminal/TerminalKeyboardLayout.swift` | 17 | Carbon; macOS gates | Current macOS keyboard input-source lookup during text composition | `Services/Terminal/` |
| `Services/Terminal/TerminalBootResolution.swift` | 987 | Foundation; macOS gates | Terminal launch resolution, Ghostty discovery, boot profiles, and profile cache | `Services/Terminal/` |
| `Services/Terminal/TerminalRenderCoordinator.swift` | 344 | Foundation; macOS gates | Render priority, wakeup coalescing, refresh scheduling, and diagnostics | `Services/Terminal/` |
| `Services/Terminal/TerminalRuntimePublicationPolicy.swift` | 46 | macOS gates | Shell-facing runtime snapshot publication policy | `Services/Terminal/` |
| `Services/Terminal/TerminalHostRuntimeReporter.swift` | 48 | Foundation; macOS gates | Runtime snapshot deduplication and main-queue publication for terminal host updates | `Services/Terminal/` |
| `Services/Terminal/TerminalHostWindowObserver.swift` | 55 | AppKit; macOS gates | Terminal host window key, screen, and occlusion notification ownership | `Services/Terminal/` |
| `TerminalRuntimeRegistry.swift` | 598 | SwiftUI; macOS gates | Pane/content-keyed terminal host/runtime registry | `Services/Terminal/` |
| `Services/Terminal/DarwinTerminalPtyRuntime.swift` | 820 | Darwin, Foundation; macOS gates | Local Darwin PTY handle, renderer proxy, nonblocking IO, process launch, and exit observation | `Services/Terminal/` |
| `Services/Terminal/GhosttyProcessBootstrap.swift` | 143 | Darwin, Foundation, GhosttyKit; macOS/Ghostty gates | Process-wide Ghostty initialization and inherited terminal-environment scrubbing | `Services/Terminal/` |
| `Services/Terminal/GhosttyTerminalSurfaceHandle.swift` | 541 | AppKit, Foundation, GhosttyKit; macOS/Ghostty gates | Ghostty surface lifecycle, PTY attachment, delivery, renderer updates, and event-engine adaptation | `Services/Terminal/` |
| `Services/Terminal/ManagedUserTerminalPtyRuntime.swift` | 764 | Darwin, Foundation; macOS gates | Privileged-helper Managed User PTY provider, handle, renderer proxy, and raw-byte bridge | `Services/Terminal/` |
| `Services/Terminal/TerminalPtyContracts.swift` | 125 | Foundation; macOS gates | PTY lifecycle, dimensions, exit, attachment, handle, runtime, and provider contracts | `Services/Terminal/` |
| `Services/Terminal/TerminalPtyControlSequenceResponder.swift` | 223 | Foundation; macOS gates | Bounded PTY control-sequence parser and terminal query responses | `Services/Terminal/` |
| `Services/Terminal/TerminalPtyRuntime.swift` | 67 | Darwin, Foundation; macOS gates | Content-keyed PTY handle registry, local/Managed User dispatch, and shared socket setup | `Services/Terminal/` |
| `Services/Terminal/TerminalRuntimeDelivery.swift` | 104 | Foundation; macOS gates | Stable terminal delivery codes and result construction | `Services/Terminal/` |
| `Services/Terminal/TerminalSurfaceContracts.swift` | 263 | AppKit, Foundation, GhosttyKit; macOS/Ghostty gates | Surface lifecycle, teardown, transcript, event-surface, and runtime-service contracts | `Services/Terminal/` |
| `Services/Terminal/TerminalTranscriptCapture.swift` | 84 | Foundation; macOS gates | Bounded live/fallback terminal transcript capture and dimension projection | `Services/Terminal/` |
| `Services/Terminal/WindowTerminalRuntimeService.swift` | 198 | Foundation; macOS gates | Window-scoped content-keyed surface ownership, restored transcripts, delivery, and teardown | `Services/Terminal/` |
| `Services/Terminal/TerminalInputRouter.swift` | 240 | Foundation; macOS gates | Shell-action lookup, focus-transfer sequencing, and keyboard/pointer routing coordination | `Services/Terminal/` |
| `Services/Terminal/TerminalKeyboardRouting.swift` | 178 | Foundation; macOS gates | Key models, IME control policy, clear-command tracking, and key-equivalent routing | `Services/Terminal/` |
| `Services/Terminal/TerminalMetadataAdapter.swift` | 80 | Foundation; macOS gates | Renderer, process, and surface-readiness overlay projection | `Services/Terminal/` |
| `Services/Terminal/TerminalPointerRouting.swift` | 191 | Foundation; macOS gates | Pointer/button normalization and terminal mouse, selection, and hover routing | `Services/Terminal/` |
| `Services/Terminal/TerminalScrollbackAdapter.swift` | 163 | Foundation; macOS gates | Terminal-mode tracking, bounded scrollback state, and native row-scroll normalization | `Services/Terminal/` |
| `Services/Terminal/TerminalSearchAdapter.swift` | 124 | Foundation; macOS gates | Pane-scoped search state and live surface search-engine contract | `Services/Terminal/` |
| `Services/Terminal/TerminalSelectionClipboardAdapter.swift` | 70 | AppKit, Foundation; macOS gates | Selection-engine contract, pasteboard writing, and guarded paste delivery | `Services/Terminal/` |
| `Services/Terminal/TerminalSemanticCommands.swift` | 97 | Foundation; macOS gates | Reliable command ranges, segments, semantic state, and command-buffer contract | `Services/Terminal/` |
| `Services/Terminal/TerminalSurfaceController.swift` | 640 | AppKit, Foundation, GhosttyKit; macOS/Ghostty gates | Surface binding, lifecycle, delivery, semantic/search coordination, readiness, and Ghostty event forwarding | `Services/Terminal/` |
| `Services/Terminal/TerminalSurfaceState.swift` | 92 | Foundation; macOS gates | Surface readiness, overlay state, and observable terminal-surface snapshots | `Services/Terminal/` |
| `Models/Shell/ShellWorkspaceValueTypes.swift` | 159 | Foundation | Shared shell commands, split/focus directions, attention, tab organization, and launch-target values | `Models/Shell/` |
| `Models/Shell/TerminalProfileModels.swift` | 337 | Foundation | Terminal Profile launch, definition, document, validation, editor, store, and resolution result DTOs | `Models/Shell/` |
| `Models/Shell/ManagedTerminalAccountModels.swift` | 38 | Foundation | Managed-account request and validation-error DTOs | `Models/Shell/` |
| `Models/Shell/TerminalActivityModels.swift` | 418 | Foundation | Terminal activity event, display, freshness, priority, progress, and snapshot values | `Models/Shell/` |
| `Models/Shell/TerminalRuntimeSnapshots.swift` | 192 | CoreGraphics, Foundation; macOS gates | Terminal host, renderer, pane metadata, and shell-projection snapshot DTOs | `Models/Shell/` |
| `Models/Shell/ShellContextSnapshot.swift` | 125 | Foundation | Process binding and shell runtime-context metadata DTOs | `Models/Shell/` |
| `Services/Shell/ManagedTerminalAccountValidation.swift` | 11 | Foundation | Fail-closed managed-account request validation through shell-core FFI | `Services/Shell/` |
| `Services/Shell/AlanPrivilegedHelperContracts.swift` | 425 | Foundation, Security | Signing identity plus typed helper status, diagnosis, plan, PTY, diagnostic, and client contracts | `Services/Shell/` |
| `Services/Shell/ManagedTerminalAccountPlanning.swift` | 413 | Foundation | Transitional macOS managed-account plan and rollback projection pending the 4.4 adapter audit | `Services/Shell/` |
| `Services/Shell/ManagedTerminalAccountEffects.swift` | 226 | Foundation | Approved helper-plan execution and local Terminal Profile effects | `Services/Shell/` |
| `Services/Terminal/TerminalAgentActivityAdapter.swift` | 177 | Foundation | Sanitized agent-event to terminal-activity projection | `Services/Terminal/` |
| `Models/Shell/ShellPaneSnapshots.swift` | 127 | Foundation | Pane identity, viewport, Alan binding, and pane-local runtime metadata DTOs | `Models/Shell/` |
| `Models/Shell/ShellContentSnapshots.swift` | 555 | CoreGraphics, Foundation | Terminal transcript, renderer state, content payload, and content-instance DTOs | `Models/Shell/` |
| `Models/Shell/ShellPaneTreeSnapshots.swift` | 492 | Foundation | Pane-slot split-tree and portable slot-tree query models | `Models/Shell/` |
| `Models/Shell/ShellTabSpaceSnapshots.swift` | 310 | Foundation | Tab, content-tab, Space identity, icon, and default-name DTOs | `Models/Shell/` |
| `Models/Shell/ShellWorkspaceSnapshots.swift` | 662 | Foundation | Workspace state snapshots and cross-family snapshot query helpers | `Models/Shell/` |
| `Models/Shell/ShellStateRuntimeSupport.swift` | 348 | Foundation | Narrow app-target shell bootstrap defaults, mutation result/error types, Terminal Profile inheritance queries, platform activity acknowledgement/projection, and inactive temporary-tab query support | `Models/Shell/` |
| `Models/Shell/ShellTitlePresentation.swift` | 371 | Foundation | Shell titles, visible labels, terminal status, and pane title-bar detail projection | `Models/Shell/` |
| `Models/Shell/ShellSidebarTabPresentation.swift` | 342 | Foundation | Sidebar tab projection, temporary-tab controls, and context-menu presentation | `Models/Shell/` |
| `Models/Shell/ShellSidebarTabDragDrop.swift` | 57 | Foundation | Sidebar tab drag payloads, insertion targets, and drop-index projection | `Models/Shell/` |
| `Models/Shell/ShellSidebarPaneTopology.swift` | 265 | Foundation | Visible pane summaries and sidebar split-topology classification | `Models/Shell/` |
| `Models/Shell/ShellActivityNotificationPresentation.swift` | 194 | Foundation | Activity attention, notification identity, visibility, and route projection | `Models/Shell/` |
| `Models/Shell/ShellSettingsSurfaceModel.swift` | 531 | Foundation; macOS gates | Settings navigation DTOs, row projection, grouping, and the composed settings snapshot | `Models/Shell/` |
| `Models/Shell/TerminalSettingsSummaries.swift` | 168 | Foundation; macOS gates | Terminal Profile, privileged-helper, and Space identity settings summaries | `Models/Shell/` |
| `Models/Shell/ManagedTerminalAccountSettingsSummary.swift` | 63 | Foundation; macOS gates | Managed-account request discovery, diagnosis, and settings summary projection | `Models/Shell/` |
| `Models/Shell/ManagedTerminalAccountCatalog.swift` | 91 | Foundation; macOS gates | Durable managed-account catalog normalization and storage | `Models/Shell/` |
| `Models/Shell/ManagedTerminalUserSettings.swift` | 240 | Foundation; macOS gates | Managed-user readiness, creation preview, validation, and approved provisioning flow | `Models/Shell/` |
| `Models/Shell/ShellSettingsHostSummaries.swift` | 131 | Foundation; macOS gates | Local app/runtime/storage identity and performance-diagnostics summaries | `Models/Shell/` |
| `ShellHostController.swift` | 383 | Foundation, Combine; macOS gates | Observable shell state, dependency assembly, startup, shutdown, and root lifecycle | Root controller with focused `Controllers/Shell/` responsibility extensions |
| `Controllers/Shell/ShellHostControlCommandHandling.swift` | 1199 | Foundation; macOS gates | Shell control-plane command dispatch and pane mutation routing | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostControlProjection.swift` | 268 | Foundation; macOS gates | Control response, list, routing-candidate, and terminal-delivery projection | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostObservationControlCommandHandling.swift` | 257 | Foundation; macOS gates | Agent activity, attention, event, render-metric, and performance-diagnostic commands | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostProjectionAndSelection.swift` | 491 | Foundation; macOS gates | Derived shell projection, focus, selection, zoom, and shell-surface close requests | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostSpaceAndTabLifecycle.swift` | 727 | Foundation; macOS gates | Space, tab, content, and split creation plus tab ordering and movement | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostActionAndTerminalCommandHandling.swift` | 494 | Foundation; macOS gates | Shell action execution, spatial focus, and terminal semantic-command routing | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostRuntimeProjection.swift` | 641 | Foundation; macOS gates | Terminal runtime and metadata projection, activity routing, state adoption, and selection synchronization | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostWorkspacePersistence.swift` | 328 | Foundation; macOS gates | Workspace-manifest projection, transcript capture, pinned snapshots, and active-task persistence | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostCloseAndPaneLifecycle.swift` | 550 | Foundation; macOS gates | Close guards, graceful shutdown, pane lifecycle/movement, and control-plane state publication | `Controllers/Shell/` |
| `Controllers/Shell/ShellHostAutomationCommandHandling.swift` | 309 | Foundation; macOS gates | External shell automation command adaptation and response projection | `Controllers/Shell/` |
| `Services/Shell/ShellControlFilePoller.swift` | 182 | Foundation; macOS gates | File-backed command/result polling and alan binding-file projection | `Services/Shell/` |
| `Services/Shell/ShellDiagnostics.swift` | 16 | Foundation; macOS gates | Shell service diagnostic routing | `Services/Shell/` |
| `Services/Shell/ShellEventStore.swift` | 677 | Foundation; macOS gates | Shell event buffering, diffing, `events.read`, and jsonl persistence | `Services/Shell/` |
| `Services/Shell/ShellLocalCommandExecutor.swift` | 922 | Foundation; macOS gates | Local shell control command execution against shell state and terminal delivery effects | `Services/Shell/` |
| `Services/Shell/ShellPaneProjectionService.swift` | 302 | Foundation; macOS gates | Pane boot context, runtime metadata, viewport, attention, and alan binding projection | `Services/Shell/` |
| `Services/Shell/ShellPublishedStateMerger.swift` | 193 | Foundation; macOS gates | Merge published shell state with authoritative runtime metadata | `Services/Shell/` |
| `Services/Shell/ShellSocketServer.swift` | 415 | Foundation, Darwin; macOS gates | Bounded local socket transport, request parsing, and client response handling | `Services/Shell/` |
| `ShellControlPlane.swift` | 463 | Foundation; macOS gates | Shell control-plane orchestration across socket, file polling, state publishing, pane support directories, event store, and diagnostics | `Services/Shell/` |

## Target Layout

The accepted target under `clients/apple/alan-macos` is:

- `App/`: `AlanApp`, app delegate, duplicate-instance startup, primary
  shell owner creation, app commands, and primary window coordination.
- `Views/Shell/`: the default macOS shell composition, sidebar, workspace,
  command palette, pane title/search UI, and shell-specific SwiftUI components.
- `Models/`: shell snapshots, shell IDs, enums, value types, and
  current-format decoding.
- `Controllers/`: observable app and shell controllers that own UI state and
  delegate IO or domain work to services. This folder currently records the
  target owner while `ShellHostController.swift` remains tracked migration debt.
- `Services/`: terminal runtime services, Ghostty bootstrap, shell projection
  services, shell control plane,
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
runtime owners. Command-failure activity acknowledgement remains a Swift
adapter-layer pass after Rust focus until the portable reducer owns that
semantic explicitly. Runtime-bearing
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
Host-required, diagnostic, render-metric, and unsupported terminal-delivery
commands stay in Swift. Primary window summon is a macOS app/window command,
outside the shell action and control registries. Settings surface row summaries for
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
  diagnostics and terminal runtime delivery separate from portable shell-domain
  commands.
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
  mutating profile documents locally. Managed-account request validation calls
  `managed_terminal_account.validate_request` through
  `Services/Shell/ShellCoreFFIManagedTerminalAccountAdapter.swift` and fails
  closed when shell-core is unavailable. Provisioning and rollback projection
  remains a transitional Swift service in
  `Services/Shell/ManagedTerminalAccountPlanning.swift`; replacing that portable
  planning path with its narrow shell-core adapter is explicit 4.4 debt.
  DTOs, helper contracts, platform effects, and activity projection now have
  separate model or service owners instead of a generalized value-types bucket.
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
- Runtime exceptions to track: command-failure activity acknowledgement remains
  Swift adapter-layer behavior until the core has explicit portable semantics
  for that case.

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

`check-architecture-maintainability.sh` currently completes in report mode.
1 known large-file / bridge-boundary warning remains after the first sixteen Apple
ownership slices removed the `ShellHostController.swift` bridge warning and
split control projection, observation commands, root-controller
responsibilities, shell presentation models, settings models, snapshot DTO
families, shell/platform value families, sidebar and settings presentation,
pane-tree/content rendering, and pane title/overlay presentation into focused
owners, then isolated Ghostty's macOS platform adapters and terminal runtime
responsibilities into focused owners, split terminal host lifecycle, pointer,
keyboard, text-input, and tracing responsibilities, separated the window
runtime service from PTY, bootstrap, surface, transcript, and test-double owners,
and split terminal-surface state, scrollback, semantic, input, search, selection,
metadata, and coordination owners. This warning is telemetry for the cleanup, not the cleanup
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

- 14 large Swift files over the report threshold.
- 1 bridge-boundary warning for `ShellHostController.swift` importing AppKit
  while still outside a narrow bridge owner.

The first Apple ownership slice moved close confirmation, app activity, and
pasteboard access behind narrow `Services/Shell` adapters. The controller now
imports only Foundation and Combine, reducing the inventory to 14 large-file
warnings with no bridge-boundary warning.

The second ownership slice moved response/list projection and observation/
diagnostic command handling out of the dispatch file. The command owner is now
1,199 lines and the current inventory is 13 large-file warnings.

The third ownership slice left `ShellHostController.swift` with observable
state, dependency assembly, startup, shutdown, and root lifecycle. Selection,
space/tab lifecycle, actions, runtime projection, persistence, close/pane
lifecycle, and automation adaptation now live in focused controller extensions;
the root is 383 lines and the current inventory is 12 large-file warnings.

The fourth ownership slice removed the root `ShellModel.swift` bucket. Title
and status text, sidebar tab projection, tab drag/drop, pane topology, and
activity notifications now have separate presentation-model owners under
`Models/Shell/`; the current inventory is 11 large-file warnings.

The fifth ownership slice reduced `ShellSettingsSurfaceModel.swift` to settings
navigation and snapshot composition. Terminal/helper summaries, managed-account
discovery, catalog storage, managed-user creation/provisioning, and local/diagnostics
summaries now have separate domain owners under `Models/Shell/`; the current
inventory is 10 large-file warnings.

The sixth ownership slice removed the generalized `ShellSnapshots.swift` bucket.
Pane primitives, content and transcript payloads, pane trees, tab/Space models,
and workspace snapshots now have separate DTO-family owners under `Models/Shell/`;
the current inventory is 9 large-file warnings.

The seventh ownership slice removed the generalized `ShellValueTypes.swift`
bucket. Shell, Terminal Profile, managed-account, activity, context, privileged-
helper, planning, and platform-effect families now live beside their durable
model or service owners. The helper fake is script-only, and the unused local
command runner is deleted; the current inventory is 8 large-file warnings.

The eighth ownership slice reduced `ShellSidebarView.swift` to sidebar
composition, Space paging, activity freshness, and tab-list orchestration.
Tab drop handling, the Space slider, tab-row chrome, activity progress, and
pane-topology presentation now have focused owners under `Views/Shell/`; the
current inventory is 7 large-file warnings.

The ninth ownership slice removed the settings surface from
`TerminalPaneView.swift`. Settings composition and managed-user actions,
navigation presentation, and reusable settings controls now have focused owners
under `Views/Shell/Settings/`. The warning count remains 7, while the hard
`TerminalPaneView.swift` ceiling falls from 3,839 to 2,513 lines ahead of the
remaining pane-tree, content, title-bar, find-bar, and terminal-leaf splits.

The tenth ownership slice moved pane-tree and split layout, terminal-leaf and
restored-transcript composition, and bounded Agent/Markdown content rendering
into focused owners under `Views/Shell/Terminal/` and `Views/Shell/Content/`.
The warning count remains 7, while the hard `TerminalPaneView.swift` ceiling
falls from 2,513 to 1,337 lines ahead of the title-bar, find-bar, and local
utility split.

The eleventh ownership slice moved responsive pane title bars and terminal
overlay presentation into focused owners under `Views/Shell/Terminal/`, and
deleted the unreferenced runtime, boot, Ghostty, binding, action, chip, and info
card presentation chain. `TerminalPaneView.swift` falls from 1,337 to 229 lines
and exits the large-file ledger, lowering the warning count from 7 to 6.

The twelfth ownership slice moved Ghostty physical key codes, clipboard access,
application-focus observation, the transparent canvas view, and display lookup
behind `Services/Terminal/GhosttyPlatformAdapters.swift`. `GhosttyLiveHost.swift`
retains app/surface lifecycle, callback assembly, and renderer coordination,
falls from 1,258 to 1,179 lines, and exits the large-file ledger, lowering the
warning count from 6 to 5.

The thirteenth ownership slice removed the generalized
`TerminalHostRuntime.swift` root. Boot resolution and profile caching, render
coordination, shell publication policy, and runtime snapshot DTOs now live in
separate `Services/Terminal/` and `Models/Shell/` owners. No replacement file
exceeds 987 lines, and the warning count falls from 5 to 4.

The fourteenth ownership slice moved `TerminalHostView.swift` into its durable
terminal-view folder and split first-responder/pointer routing, physical keyboard
translation, `NSTextInputClient`, keyboard-layout lookup, and input tracing into
focused `Services/Terminal/` adapters. The lifecycle owner is 574 lines, no
replacement file exceeds 637 lines, and the warning count falls from 4 to 3.

The fifteenth ownership slice removed the generalized
`TerminalRuntimeService.swift` root. Delivery and PTY contracts, control-sequence
responses, local and Managed User PTY implementations, Ghostty bootstrap and
surface adaptation, transcript capture, and window-scoped runtime ownership now
live in focused `Services/Terminal/` files. Runtime fakes moved out of the app
target into `scripts/support/TerminalRuntimeTestDoubles.swift`; no production
replacement exceeds 820 lines, and the warning count falls from 3 to 2.

The sixteenth ownership slice removed the generalized
`TerminalSurfaceController.swift` root. Scrollback and terminal-mode state,
semantic command ranges, keyboard and pointer routing, input coordination,
search, selection and clipboard behavior, surface state, metadata projection,
and lifecycle coordination now live in focused `Services/Terminal/` owners. The
coordinator is 640 lines, no other replacement exceeds 240 lines, and the
warning count falls from 2 to 1.

Rust-owned Swift legacy cleanup targets at this baseline:

| File | Lines | Cleanup target |
| --- | ---: | --- |
| `Models/Shell/ShellWorkspaceManifest.swift` | 513 | Cleaned: Swift manifest default/prune/materialize/migration parity implementations and fixture exporters are removed. Production keeps DTOs, repair, transcript cleanup, projection helpers, and file IO while Rust manifest contract tests own portable behavior. |
| `Models/Shell/ShellActionRegistry.swift` | 248 | Cleaned: the Swift standard action registry table and resolver fixture are removed. Production keeps action IDs, targets, effects, keyboard action values, and terminal command target resolution while Rust action tests plus FFI adapter tests own registry behavior. |
| `Models/Shell/ShellStateRuntimeSupport.swift` | 348 | Cleaned: production no longer compiles `ShellStateMutations.swift` or `ShellTreeMutations.swift`, and the script-side duplicate Swift reducer/tree implementations are removed. The app target keeps only bootstrap defaults, mutation result/error shapes, Terminal Profile inheritance queries, platform activity acknowledgement/projection, and inactive temporary-tab query support. Script state construction that still needs mutations uses the FFI-backed `ShellCoreFFITestStateBuilder.swift`. |
| `Models/Shell/ShellValueTypes.swift` | Removed | Cleaned: shell, Terminal Profile, managed-account, activity, context, helper-contract, planning, and effect families now have named model/service owners. Request validation calls shell-core FFI and fails closed. The script-only helper fake is outside the app target, and the unused local command runner is deleted. Portable provisioning/rollback projection remains explicit 4.4 adapter-audit debt in `Services/Shell/ManagedTerminalAccountPlanning.swift`. |
| `Models/Shell/ShellSettingsSurfaceModel.swift` | 531 | Cleaned below the report threshold: settings navigation/grouping and fail-closed row composition remain here, while Terminal Profile/helper summaries, managed-account discovery, catalog persistence, managed-user creation/provisioning, and local/diagnostics summaries live in adjacent domain modules. Managed terminal account row projection still calls `settings.managed_terminal_account_rows` through `Services/Shell/ShellCoreFFISettingsAdapter.swift`. |
| `Services/Shell/ShellCoreFFIAdapter.swift` | 12 | Cleaned as adapter facade: loader, envelope, materialization, manifest, reducer, control, action, settings, Terminal Profile, and managed terminal account operation owners now live in sibling `ShellCoreFFI*` files; no Swift domain fallback was added. |
| `ShellHostController.swift` | 383 | Cleaned below the report threshold: the root keeps observable state, dependency assembly, startup, shutdown, and root lifecycle. Selection, space/tab lifecycle, actions, runtime projection, persistence, close/pane lifecycle, and automation adaptation live in focused `Controllers/Shell/ShellHost*` extensions while Rust shell-core and existing service coordinators retain domain authority. |
| `Controllers/Shell/ShellHostControlCommandHandling.swift` | 1,199 | Cleaned below the report threshold: reusable reducer invocations route through `ShellReducerCommandCoordinator`, while response/list projection and observation/diagnostic commands live in adjacent controller modules. The dispatch owner retains control-command routing and pane mutation coordination without a second Rust-owned mutation implementation. |

The first implementation batch targeted the production Swift legacy surface, not
a line-count threshold. Manifest parity helpers and the Swift standard action
registry plus reducer/tree mutation parity support moved out of normal
app-target sources into script support. Direct inspection of
`ShellHostControlCommandHandling.swift` shows no direct shell-core FFI owner or
Swift reducer fallback remains there. Its response projection and observation
commands now have adjacent owners, while the remaining dispatch and pane
mutation routing stays on the platform side of the Rust-domain boundary. The
reduced architecture warning count is a byproduct; success is defined by the
absence of production-compiled Rust-owned Swift implementations and by
`check-shell-contracts.sh` continuing to reject fallback paths.

The current architecture gate treats
`clients/apple/scripts/architecture-warning-baseline.txt` as a hard downward
ratchet. The report must exactly match its structured warning inventory, and
the quality gate compares that inventory with the pre-change commit. New or
broadened warnings and increases to an existing large-file ceiling fail even
when code and ledger are updated together. A reduction must tighten the ledger
and this count in the same change.

The gate also fails narrower regressions such as new root-level Swift files,
project membership drift, reintroduced control-plane ownership in the wrong
file, direct reducer FFI calls outside `ShellReducerCommandCoordinator`, or
direct shell-core FFI calls outside the documented operation owner allowlist.
Direct `ShellCoreFFIAdapter` construction is likewise restricted to the loader
owner, and raw `alan_shell_core_ffi_*` symbol access must stay in that same
loader owner, so production code cannot bypass the shared adapter boundary.
