# macos-shell-build-test-contract Specification

## Purpose
Define the Apple client build, dependency, and focused test contract for the
macOS shell host.
## Requirements
### Requirement: Build requirements match documentation
The Apple client SHALL keep documented system requirements, deployment targets,
project settings, project naming, scheme naming, and build commands aligned
with the active `alan for macOS` engineering identity.

#### Scenario: Deployment target changes
- **WHEN** the Xcode project deployment targets are changed
- **THEN** `clients/apple/README.md` and relevant specs are updated in the same
  change

#### Scenario: Documented build command
- **WHEN** a developer runs the documented macOS build command after preparing
  dependencies
- **THEN** the command succeeds or fails with documented, actionable dependency
  setup instructions

#### Scenario: Project or scheme name changes
- **WHEN** the Apple project, source root, target, scheme, or generated product
  name changes
- **THEN** `clients/apple/README.md`, architecture docs, focused scripts, active
  OpenSpec tasks, and Xcode project settings are updated in the same change
- **AND** no active documented build command references `AlanNative`

### Requirement: Ghostty dependency setup is explicit
The Apple project SHALL treat Ghostty framework, resources, and terminfo as an
explicit local dependency with a verifiable setup path.

#### Scenario: Dependencies are missing
- **WHEN** `GhosttyKit.xcframework`, `ghostty-resources`, or `ghostty-terminfo` are absent
- **THEN** the build or setup check reports the missing dependency and points to the supported preparation command

#### Scenario: Dependencies are present
- **WHEN** local Ghostty artifacts are prepared
- **THEN** the macOS app build links/copies them without module-map or umbrella-header warnings that obscure real failures

### Requirement: Shell model behavior has focused tests
The Apple client SHALL have focused automated tests for shell state mutation and
control-plane behavior that can run without launching the full app UI.

#### Scenario: State mutation tests
- **WHEN** shell spaces, tabs, and panes are created, split, moved, lifted, focused, and closed
- **THEN** tests verify focused IDs, pane trees, space membership, attention state, and failure cases

#### Scenario: Control-plane tests
- **WHEN** control-plane query and mutation commands are executed against a test host
- **THEN** tests verify successful responses, missing-target errors, event records, and text-delivery acknowledgement semantics

### Requirement: Terminal host boundary is testable
The terminal host SHALL expose a testable boundary for runtime attachment,
teardown, and text delivery without requiring the real Ghostty library in every
test.

#### Scenario: Mock runtime accepts text
- **WHEN** a test runtime is registered for terminal content and `terminal.send_text` is issued
- **THEN** the test verifies the text reaches the runtime and the control response reports accepted bytes with the terminal `content_id`

#### Scenario: Mock runtime unavailable
- **WHEN** no runtime is registered for terminal content and `terminal.send_text` is issued
- **THEN** the test verifies the response reports failure or durable queueing according to the delivery contract

### Requirement: Surface behavior has focused verification
The Apple client SHALL add focused tests or documented manual verification for
terminal scrollback, input translation, IME/preedit, selection, clipboard,
search, terminal mode changes, renderer health, and child-exit behavior.

#### Scenario: Scrollback verification
- **WHEN** terminal surface work changes scrollback or scrollbar behavior
- **THEN** tests or manual notes verify normal-buffer scrolling, alternate-screen behavior, and scrollbar synchronization

#### Scenario: Input verification
- **WHEN** terminal input adapter behavior changes
- **THEN** tests or manual notes verify printable input, command-key routing, modifiers, IME composition, paste, and terminal mouse mode

#### Scenario: Failure-state verification
- **WHEN** renderer health, child-exit, or fallback UI changes
- **THEN** tests or manual notes verify that the default UI is truthful and debug details remain inspector-only

### Requirement: Surface adapters are unit-testable with fakes
Terminal surface controllers and input/scrollback adapters SHALL be testable
with fake surface handles for state transitions and event translation that do
not require a live Ghostty renderer.

#### Scenario: Fake scroll metrics
- **WHEN** a fake surface publishes scrollback metrics
- **THEN** adapter tests verify native scrollbar range and visible viewport updates

#### Scenario: Fake input events
- **WHEN** adapter tests send keyboard, mouse, paste, and search commands through fake events
- **THEN** the fake surface receives normalized terminal operations or command-routing decisions

### Requirement: Runtime service ownership has focused tests
The Apple client SHALL include focused tests for process bootstrap, window
runtime service ownership, terminal ContentInstance handle creation, reattachment,
text delivery, and teardown using fake Ghostty adapters where possible.

#### Scenario: Fake runtime reattaches view
- **WHEN** a test creates a terminal ContentInstance handle, detaches the host view, and attaches a replacement host view
- **THEN** the test verifies that the `content_id` handle identity and runtime metadata remain unchanged

#### Scenario: Fake runtime tears down once
- **WHEN** a test closes a terminal ContentInstance, PaneSlot, tab, or window through shell actions
- **THEN** the fake runtime observes exactly one teardown call per affected terminal ContentInstance

### Requirement: Ghostty bootstrap is testable without launching the full app
The Apple client SHALL expose a bootstrap seam that lets tests verify Ghostty
dependency and initialization behavior without launching the full SwiftUI app or
requiring real terminal rendering.

#### Scenario: Bootstrap dependency missing
- **WHEN** a fake bootstrap reports missing Ghostty resources
- **THEN** tests verify that pane creation enters a non-ready state with an actionable error

#### Scenario: Bootstrap reused
- **WHEN** two window runtime services request terminal support in one test process
- **THEN** tests verify that the process bootstrap is invoked once and both services receive the same bootstrap result

### Requirement: Control-plane runtime tests use the service boundary
Control-plane tests SHALL exercise runtime-dependent mutations through the same
terminal runtime service boundary used by production code.

#### Scenario: Service accepts text
- **WHEN** a control-plane test sends text to fake live terminal content with `terminal.send_text`
- **THEN** the command response reports accepted bytes from the fake service and shell diagnostics remain clean

#### Scenario: Service reports runtime missing
- **WHEN** a control-plane test sends text to terminal content whose service handle is absent
- **THEN** the command response reports a stable runtime-missing error

### Requirement: Terminal event ownership is contract-checked
The Apple client SHALL include focused shell contract checks that preserve the
terminal event ownership boundary between SwiftUI layout, AppKit terminal host
input, rendering canvases, and native window background dragging.

#### Scenario: SwiftUI terminal tap wrapper is reintroduced
- **WHEN** a code change wraps the terminal native view in a SwiftUI tap gesture for pane selection
- **THEN** the shell contract check fails with an error explaining that terminal-area selection belongs to the terminal host

#### Scenario: Activation delegate strongly retains controller state
- **WHEN** a code change stores terminal activation as a strong registry-owned closure
- **THEN** the shell contract check fails or the focused review checklist requires replacing it with the weak activation boundary

#### Scenario: Rendering canvas becomes interactive owner
- **WHEN** a code change lets Ghostty or fallback rendering canvas views receive terminal mouse-down hit tests as independent owners
- **THEN** the shell contract check fails or the focused review checklist requires routing those events through the terminal host

#### Scenario: Focused manual verification is performed
- **WHEN** event ownership implementation is ready for review
- **THEN** verification covers click-to-select, immediate typing, drag selection, right click, scrolling, and background window dragging in the running macOS app

### Requirement: Pane title bars have focused verification
The Apple client SHALL include focused automated or documented verification for
pane title-bar consumption, pane-scoped close routing, terminal input
ownership, selected-title readability, responsive accessory layout, and
terminal-surface integration when pane title bars are changed.

#### Scenario: Pane title-bar consumption tested
- **WHEN** pane title-bar helpers receive existing terminal title, working-directory, cwd, launch-target, and process metadata combinations
- **THEN** focused tests verify title-bar priority, fallback ordering, long-title handling, and suppression of raw pane IDs or debug terms without retesting terminal title capture itself

#### Scenario: Pane close routing tested
- **WHEN** a title-bar close action targets a selected pane, an inactive split pane, a single-pane tab with other tabs, or the final remaining pane
- **THEN** focused tests verify the shell mutation result, selected pane after close, split tree repair, and final-pane protection

#### Scenario: Terminal input preservation reviewed
- **WHEN** pane title-bar implementation is ready for review
- **THEN** maintainers can inspect automated shell contract checks or manual notes covering terminal click-to-focus, typing, selection drag, right click, scrolling, and close-button interaction

#### Scenario: Title readability guarded
- **WHEN** pane title-bar UI polish changes selected or unfocused title styling
- **THEN** focused checks or documented visual review verify that the focused title remains visible as text against the terminal surface background in light mode
- **AND** contract checks fail or review blocks the change if selected title-bar styling can hide, wash out, or replace the title text with icon-only content

#### Scenario: Responsive layout guarded
- **WHEN** pane title-bar layout changes
- **THEN** focused checks or review verify that title-bar accessories use fit-content layout and staged responsive fallback instead of fixed-width accessory columns
- **AND** narrow title bars preserve title text and close affordance while lower-priority accessories degrade first

#### Scenario: Terminal-surface integration guarded
- **WHEN** pane title-bar background or material roles change
- **THEN** focused checks or documented visual review verify that the title bar matches the terminal surface background and does not reintroduce a selected/unselected overlay band above the pane

#### Scenario: Window chrome and collapsed sidebar guardrails run
- **WHEN** hidden-titlebar window chrome, titlebar double-click behavior, local app launch behavior, or collapsed-sidebar floating-panel behavior changes
- **THEN** focused checks verify launch presents one primary window, empty titlebar double-click zoom targets only non-control chrome, system traffic-light buttons keep their normal behavior, and collapsed-sidebar reveal uses narrow hover targets with stable workspace geometry

#### Scenario: Visual evidence captured
- **WHEN** pane title-bar UI polish is marked complete
- **THEN** maintainers can inspect a running-app screenshot or manual note showing light-mode single-pane and split-pane tabs with compact pane title bars, readable titles, restrained close buttons, responsive accessory fallback, and no default debug labels

### Requirement: Corner-radius conformance is verified
The Apple client SHALL include focused verification for active-shell
corner-radius normalization when default macOS shell chrome is changed.

#### Scenario: Active shell radius check runs
- **WHEN** a change updates active shell visual chrome in `MacShellRootView.swift`, `TerminalPaneView.swift`, or normal-flow `TerminalHostView.swift` fallback surfaces
- **THEN** a focused check or review step verifies that rounded rectangles use the alan shell radius scale and do not introduce large ad hoc radii

#### Scenario: Capsule usage reviewed
- **WHEN** a change adds `Capsule` usage to active default shell chrome
- **THEN** the change documents why the component is a semantic pill or replaces it with a radius-scale rounded rectangle

#### Scenario: Visual comparison captured
- **WHEN** radius normalization implementation is marked complete
- **THEN** maintainers can inspect running-app screenshots or notes for sidebar, terminal, command input, and remaining default-shell overlay states confirming that the UI is smaller-radius, still native, and not visually flat

#### Scenario: Legacy surfaces scoped
- **WHEN** radius inventory finds older or non-primary Apple client surfaces
- **THEN** implementation records whether they are active default shell UI before changing them, instead of silently broadening the polish pass

### Requirement: Find UI has focused verification
The Apple client SHALL include focused automated or documented verification for
the pane-scoped native Find bar and removed inspector surface.

#### Scenario: Find bar behavior verified
- **WHEN** Find behavior changes
- **THEN** focused tests or manual notes cover `Command-F`, query editing, Return, `Command-G`, Shift-`Command-G`, Escape, and printable query text behavior while Find is active

#### Scenario: Inspector removal contract checked
- **WHEN** default shell UI changes
- **THEN** shell contract checks fail if removed inspector UI, commands, voice phrases, or stale inspector affordances are reintroduced

#### Scenario: Passive search overlay blocked
- **WHEN** terminal search UI changes
- **THEN** shell contract checks fail if default Find UI is routed through the passive terminal overlay instead of `ShellFindBarView`

### Requirement: Apple architecture maintainability has focused validation
The Apple client SHALL provide focused validation for source layout,
multi-responsibility hotspots, README/project drift, and SwiftUI/AppKit boundary
regressions when architecture maintainability changes are implemented.

#### Scenario: Architecture report is run
- **WHEN** a developer runs the Apple architecture-maintainability check
- **THEN** the report identifies files or project groups that violate the
  accepted source ownership boundaries and gives actionable paths to the owning
  files or folders

#### Scenario: README and project layout drift
- **WHEN** Apple client source folders or Xcode project groups are reorganized
- **THEN** validation or review confirms `clients/apple/README.md`, source
  paths, and project membership describe the same structure

#### Scenario: AppKit bridge spreads into unrelated SwiftUI
- **WHEN** a change introduces `NSWindow`, `NSView`, `NSApp`, Darwin socket, or
  process-management ownership into a SwiftUI feature view that does not own a
  platform bridge
- **THEN** the architecture check fails or the review checklist requires moving
  the behavior into an app, service, terminal host, or support boundary

#### Scenario: Behavior-preserving move is reviewed
- **WHEN** a refactor slice only moves or extracts Apple client code
- **THEN** verification includes diff review, project membership validation, and
  the focused Apple build or script checks needed to prove behavior was not
  intentionally changed

### Requirement: Streamlined sidebar has focused verification
The Apple client SHALL include focused verification for sidebar information
architecture changes that remove visible copy or restructure space/tab
navigation.

#### Scenario: Sidebar reading order is reviewed
- **WHEN** sidebar IA implementation is marked complete
- **THEN** maintainers can inspect screenshots or manual notes showing the vertical sidebar, active-space tab list, bottom borderless space switcher, separate creation affordances, and no persistent explanatory sidebar blocks

#### Scenario: Sidebar interaction states are reviewed
- **WHEN** tab or space row secondary actions are progressively disclosed
- **THEN** verification covers default, hover, selected, no-notification-dot, and empty states without row resizing or layout shifts
- **AND** verification covers the tab row hierarchy where normal rows are containerless, hover/focus rows are subtle, selected rows are strongest, and creation rows stay muted until interaction
- **AND** verification covers the space-title/tab-list scroll boundary where the divider and shadow appear only as tab rows scroll underneath the fixed space title region

#### Scenario: Sidebar space swipe is reviewed
- **WHEN** horizontal space switching is implemented in the sidebar
- **THEN** verification covers gesture-tracked left and right sidebar previews, translation-first drag mapping, fast horizontal flick commit, full-width space-title and tab-list motion, stationary hold, zero-delta release, horizontal/vertical axis lock, later vertical movement during a horizontal swipe, no static side padding gaps during page motion, a stable workspace during drag, commit, cancel, edge resistance, and confirms vertical tab-list scrolling still works

#### Scenario: Split tab indicator is reviewed
- **WHEN** split-aware tab row implementation is marked complete
- **THEN** verification covers single-pane, two-pane horizontal, two-pane vertical, complex split, focused-pane, no-notification-dot, pointer activation, and keyboard or accessibility activation states

#### Scenario: Accessibility copy is preserved
- **WHEN** visible sidebar text is removed or shortened
- **THEN** review confirms accessibility labels, help text, menu labels, or equivalent nonvisual descriptions still identify the affected controls

### Requirement: Command input polish has focused verification
The Apple client SHALL include focused verification for the `Command-P` input
surface when command UI behavior or material treatment changes.

#### Scenario: Command input keyboard flow is verified
- **WHEN** command input implementation is marked complete
- **THEN** focused tests or manual notes cover open/focus, typing, successful Return submission, unresolved Return behavior, Escape dismissal, click-away dismissal, and terminal focus restoration

#### Scenario: Candidate sections stay removed
- **WHEN** default command input UI changes
- **THEN** shell contract checks or review notes confirm action, routing, attention, best-match, command-row, and microphone affordances are not visible in the default command input surface

#### Scenario: Liquid input visual review is captured
- **WHEN** command input material polish is marked complete
- **THEN** maintainers can inspect screenshots or manual notes showing the input over the active light-mode shell with legible text, restrained depth, and no large panel below the field

### Requirement: Material hierarchy has focused verification
The Apple client SHALL include focused verification for active-shell material
changes so native material polish does not reduce terminal readability or
reintroduce hard-coded visual effects.

#### Scenario: Material review is captured
- **WHEN** active macOS shell material roles, background surfaces, or compact control treatments change
- **THEN** maintainers can inspect screenshots or manual notes covering the default light-mode sidebar, terminal content area, command entry, compact controls, and floating overlays

#### Scenario: Accessibility material settings are reviewed
- **WHEN** material hierarchy implementation is marked complete
- **THEN** verification includes reduced-transparency or increased-contrast review notes, or a documented reason those settings could not be exercised locally

#### Scenario: One-off material fills are checked
- **WHEN** a change adds new active-shell material or translucent fills
- **THEN** focused review or a lightweight check confirms the fill is attached to a shared semantic material/control role rather than a local hard-coded effect

#### Scenario: Elevation hierarchy is reviewed
- **WHEN** active macOS shell radius, shadow, rim, or floating-surface treatment changes
- **THEN** focused review confirms terminal surface, sidebar selection, titlebar controls, command launcher, Find bar, command input, and collapsed sidebar panel use the shared semantic radius/elevation scale

#### Scenario: Light-mode shadow cleanliness is reviewed
- **WHEN** active shell elevation changes are marked complete
- **THEN** maintainers can inspect screenshots or notes confirming light-mode shadows are focused and adaptive rather than broad, dirty, or purely black halos

### Requirement: Branding and project identity checks run with Apple validation
Apple-client validation SHALL include focused checks that protect the canonical
`Alan` product brand, `Alan for macOS` platform label, and `alan-macos`
engineering identity.

#### Scenario: Brand scan runs
- **WHEN** Apple-client validation runs for a branding or project rename change
- **THEN** it scans active Apple source, scripts, docs, project metadata, and
  active OpenSpec changes for non-allowlisted `AlanNative`, `Alan Shell`,
  `alanterm`, `dev.alan.native`, `alan.app`, `alan for macOS`, and lowercase
  generated app metadata occurrences
- **AND** it reports the expected canonical replacement for each violation

#### Scenario: Renamed Xcode build runs
- **WHEN** implementation is ready for review
- **THEN** the documented Xcode build command uses
  `clients/apple/alan-macos.xcodeproj`, scheme `alan-macos`, configuration
  `Debug`, destination `generic/platform=macOS`, and the shared derived-data path
- **AND** the build produces `Alan.app`

#### Scenario: Focused scripts are updated
- **WHEN** focused Apple shell scripts are run after the rename
- **THEN** they read source files from `clients/apple/alan-macos`
- **AND** script defaults such as bundle identifiers, capture helpers, and
  architecture checks use the current app identity instead of `AlanNative` or
  `dev.alan.native`

### Requirement: Workspace persistence verification covers Tab lifecycle
Changes to macOS shell workspace persistence SHALL include focused verification for manifest startup, Space retention, Pinned Tab restore snapshots, Unpinned Tab TTL retirement, and active-task retirement protection.

#### Scenario: Manifest startup behavior is tested
- **WHEN** workspace persistence changes are implemented
- **THEN** focused tests cover missing manifest default creation and corrupt manifest quarantine with fresh default startup

#### Scenario: Space retention is tested
- **WHEN** tab close or lifecycle retirement can leave a Space without Tabs
- **THEN** focused tests or manual notes verify the Space remains visible and selected with an empty workspace state

#### Scenario: Pinned Tab restore is tested
- **WHEN** Pinned Tab persistence is implemented
- **THEN** focused tests cover single-pane cwd restoration, split layout restoration, and the fact that post-pin transient split/cwd changes do not update the pin snapshot without an explicit update-pin action

#### Scenario: Unpinned Tab TTL is tested
- **WHEN** Unpinned Tab lifecycle pruning is implemented
- **THEN** focused tests cover retained Tabs inside the 12 hour TTL, retired inactive Tabs after the TTL, and selection repair when the selected Tab is retired

#### Scenario: Active tasks are tested
- **WHEN** terminal-aware active-task metadata is used for pruning
- **THEN** focused tests cover foreground command protection, alan pending/yield protection, and idle shell eligibility for retirement

### Requirement: Sidebar interaction refinement has focused verification
The Apple client SHALL include focused automated checks or documented manual
verification for sidebar selection/focus convergence, sidebar-local space pager
behavior, and coordinated sidebar/window-chrome motion when those interactions
are changed.

#### Scenario: Sidebar selection convergence tested
- **WHEN** sidebar tab or space selection behavior changes
- **THEN** focused tests verify that selecting a tab or space updates shell focused pane, selected tab, selected space, and terminal runtime focus consistently
- **AND** tests or contract checks cover the case where runtime metadata arrives immediately after selection without reverting to the previous tab

#### Scenario: Sidebar-local space pager gesture tested
- **WHEN** horizontal space swipe behavior changes
- **THEN** focused tests cover undecided-axis buffering, horizontal intent lock, vertical scroll pass-through, stable five-page rendering around the source space, one-page-plus-overdrag drag clamping, edge resistance, commit threshold, cancel threshold, phaseful release, phase-less idle release, and fast flick velocity commit
- **AND** verification confirms only the sidebar active-space header and tab list move during the gesture
- **AND** verification confirms the command input, bottom space switcher, sidebar chrome, traffic lights, and workspace terminal surface remain fixed during the gesture

#### Scenario: Pinned sidebar motion reviewed
- **WHEN** pinned sidebar collapse or expansion behavior changes
- **THEN** maintainers can inspect automated invariants, screenshots, or manual notes showing that the sidebar surface, workspace inset, titlebar controls, and standard macOS traffic-light controls move as one coordinated transition
- **AND** verification covers the revealed-floating-sidebar to pinned-sidebar morph and confirms there is no intermediate hidden/offscreen/duplicated sidebar frame
- **AND** focused checks or contract checks confirm the presentation model, not independent pinned/floating booleans alone, drives the sidebar surface and window chrome values used during that morph

#### Scenario: Floating sidebar chrome reviewed
- **WHEN** collapsed floating-sidebar reveal or hide behavior changes
- **THEN** focused checks or manual notes verify narrow edge hover, window-level hover retention, stable terminal workspace geometry, native traffic-light behavior, and no visible traffic-light jump from the non-floating corner
- **AND** verification covers a visible-frame-zoomed window where the pointer moves through the left window resize frame without causing the revealed floating sidebar to auto-hide
- **AND** verification confirms that native window resizing still works from the left resize frame

### Requirement: Release installation replaces the debug app runner
The Apple client build/test contract SHALL treat release-shaped installation as
the supported local app workflow. The repository MUST NOT require or preserve a
`just app` workflow that force-kills and relaunches the macOS app.

#### Scenario: Local app workflow is validated
- **WHEN** Apple client workflow checks inspect the justfile and app scripts
- **THEN** they verify `just install` is the documented local app installation path
- **AND** they verify the justfile does not expose a recipe named `app`
- **AND** they verify the justfile does not expose a replacement debug app runner recipe for the same force-rebuild-and-launch workflow

#### Scenario: Legacy debug runner is removed
- **WHEN** Apple client contract checks inspect app runner scripts
- **THEN** they do not require `clients/apple/scripts/run-alan-debug-app.sh` as the supported app workflow
- **AND** they fail if a default local app workflow kills a running `Alan.app` process and immediately relaunches it

### Requirement: Release packaging has focused validation
The Apple client SHALL provide focused validation for the release app package,
embedded CLI binary, Developer ID signatures, and publication readiness
when distribution packaging changes.

#### Scenario: Release app layout is checked
- **WHEN** release packaging implementation is ready for review
- **THEN** focused checks verify `Alan.app` was built in Release configuration
- **AND** focused checks verify embedded `Contents/Resources/bin/alan` exists and is executable
- **AND** focused checks verify the embedded binary is the release binary from the current build
- **AND** focused checks verify the package manifest SHA-256 value is recorded after embedded binary signing and matches the delivered embedded binary

#### Scenario: Signatures are checked
- **WHEN** release packaging implementation is ready for review
- **THEN** focused checks verify the embedded CLI is signed with the configured Developer ID Application identity
- **AND** focused checks verify the app bundle is signed after embedded binaries are in place
- **AND** focused checks fail if ad-hoc signatures are used for local install or release artifacts

#### Scenario: Publication readiness is checked
- **WHEN** an artifact is intended for Homebrew cask or direct public download
- **THEN** focused checks verify notarization and stapling completed successfully
- **AND** focused checks verify the cask metadata links the embedded CLI from the installed app bundle

### Requirement: macOS shell documentation uses OpenSpec as the contract source
The macOS shell build and verification contract SHALL prevent active macOS
shell documentation from treating `docs/spec/` or task-specific plan files as
the authoritative UI, interaction, build, lifecycle, or runtime contract.

#### Scenario: macOS shell documentation references contracts
- **WHEN** active macOS shell README, architecture, build, install, or
  verification docs reference UI, interaction, lifecycle, runtime, distribution,
  or build/test contracts
- **THEN** they point to the relevant `openspec/specs/` capability or active
  `openspec/changes/` artifact
- **AND** they do not point to `docs/spec/`, `plans/`, or `docs/superpowers/`
  as an authoritative contract source

#### Scenario: macOS shell contract references are checked
- **WHEN** macOS shell documentation or build/test metadata is updated
- **THEN** focused validation checks for stale active references to retired
  macOS shell contract paths
- **AND** any required compatibility bridge clearly states that OpenSpec wins

### Requirement: Split topology indicators have focused verification
The Apple client SHALL include focused automated or documented verification for
sidebar split topology classification and visual stability when the split
topology indicator changes.

#### Scenario: Topology classification is tested
- **WHEN** split topology indicator logic is implemented or changed
- **THEN** focused tests verify single pane, two columns, two rows, three columns, three rows, three-pane main-plus-stack variants, four-pane recognized layouts, and complex-count fallback classification

#### Scenario: Complex count rendering is verified
- **WHEN** a tab's split topology falls back to complex count
- **THEN** focused tests or visual evidence verify that the count overlays a single-pane-shaped topology base rather than rendering beside the indicator as adjacent text or a separate badge

#### Scenario: Sidebar indicator visuals are reviewed
- **WHEN** split topology indicator UI implementation is marked complete
- **THEN** maintainers can inspect running-app screenshots or manual notes covering light-mode selected, hover, focused-pane, three-pane, four-pane, and complex-count tab-row states without row resizing or layout shifts

### Requirement: Terminal interaction regressions have focused verification
The Apple client SHALL include focused automated tests, shell contract checks,
or documented manual verification for terminal keyboard delivery, tab cwd
inheritance, and shell child-exit lifecycle changes.

#### Scenario: TUI keyboard verification
- **WHEN** terminal input routing is changed
- **THEN** verification covers Vim or an equivalent TUI receiving Escape, Tab, Backspace, control-key navigation, printable input, and command-mode transitions in a focused terminal pane

#### Scenario: Physical keyboard and programmatic text stay separate
- **WHEN** terminal input routing is changed
- **THEN** verification proves printable physical keys can enter AppKit text interpretation for IME startup while Escape and Control keys remain terminal-owned
- **AND** verification proves committed printable physical input uses terminal key-event delivery
- **AND** static checks prevent `TerminalHostView.keyDown` from calling the programmatic text injection path

#### Scenario: Native command routing verification
- **WHEN** terminal keyboard routing is changed
- **THEN** verification covers app-reserved `Command` shortcuts and visible command-input keys so terminal input ownership does not break native macOS commands

#### Scenario: AppKit responder-chain verification
- **WHEN** terminal keyboard routing is changed
- **THEN** verification covers `performKeyEquivalent`/`doCommand` redispatch for Control or Command key equivalents
- **AND** verification covers Ghostty's special `Control-/` handling and focus-only split click/drag sequences that must not reach Vim mouse mode or terminal selection
- **AND** verification covers the terminal input router as the single owner of primary pointer sequence policy instead of only testing separate focus-click or pointer helpers

#### Scenario: GhosttyKit modulemap verification
- **WHEN** local GhosttyKit artifacts are prepared for the Apple client build
- **THEN** the setup script normalizes generated GhosttyKit module maps to use `header "ghostty.h"` instead of `umbrella header "ghostty.h"`
- **AND** shell contract checks reject cached GhosttyKit module maps that would cause Clang umbrella-header warnings for internal `ghostty/vt/*` headers

#### Scenario: Terminal input trace can be toggled live
- **WHEN** terminal input routing diagnostics are enabled or disabled through user defaults
- **THEN** alan refreshes the terminal input trace configuration while running without requiring an app restart
- **AND** shell contract checks prevent the trace helper from caching the user-defaults enabled state for the whole process lifetime

#### Scenario: New tab cwd verification
- **WHEN** terminal tab creation is changed
- **THEN** verification covers runtime cwd metadata, pane snapshot cwd fallback, explicit control-plane cwd, and default/home fallback

#### Scenario: Exit lifecycle verification
- **WHEN** child-exit handling is changed
- **THEN** verification covers `exit` from a split pane, `exit` from a single-pane tab, final-pane fallback behavior, direct surface close-request forwarding, and rejection of later text delivery to an exited runtime

### Requirement: Shell Action Registry Is Verified
The Apple client SHALL include focused verification for macOS shell action
registry coverage, target resolution, availability, and shortcut conflicts.

#### Scenario: Action IDs are unique
- **WHEN** shell action registry tests run
- **THEN** every registered shell action has a unique stable action ID

#### Scenario: Shortcut conflicts are rejected
- **WHEN** two enabled shell actions in the same keyboard context declare the
  same default shortcut
- **THEN** focused verification fails with enough detail to identify both
  conflicting action IDs

#### Scenario: Context target is preserved
- **WHEN** a context menu action targets a non-selected Tab
- **THEN** focused verification proves the action resolves the context target
  and does not first select the Tab

#### Scenario: Command UI remains unchanged
- **WHEN** the shell action registry is introduced
- **THEN** focused checks confirm new Tab and Space organization actions are not
  added to `Go to or Command...` by this change

### Requirement: Keybinding System Is Verified
The Apple client SHALL include focused verification for default shortcut
registry descriptors, conflict detection, target semantics, menu hints, and input
precedence.

#### Scenario: Existing shortcuts are preserved
- **WHEN** registry-backed keybinding descriptors are introduced
- **THEN** tests prove existing shell shortcuts keep their previous key
  equivalents unless a migration is explicitly documented

#### Scenario: Conflicts are detected
- **WHEN** default shortcut descriptors are validated
- **THEN** tests fail on duplicate shortcuts in the same dispatch context and
  include both action IDs in the failure

#### Scenario: Menu hints are verified
- **WHEN** a menu item is backed by an action with a default shortcut descriptor
- **THEN** verification proves the native menu hint comes from the registry
  descriptor

#### Scenario: Keyboard target is verified
- **WHEN** keyboard dispatch invokes Tab or pane actions
- **THEN** tests prove the target is the current selected Tab or focused pane
  rather than a hovered or context-menu row

#### Scenario: Space shortcut scope is verified
- **WHEN** first-version Space shortcut coverage is tested
- **THEN** verification covers next Space, previous Space, numeric Space
  selection, and the absence of default shortcuts for create, rename, and delete
  Space

#### Scenario: Input precedence is verified
- **WHEN** Find is active or terminal input owns a key sequence
- **THEN** tests or script checks prove those handlers take precedence over
  shell action shortcut dispatch

#### Scenario: No customization surface is verified
- **WHEN** the first-version keybinding system is reviewed
- **THEN** tests or code review checklists confirm no shortcut customization UI,
  config file, manifest field, or Command UI integration was added

### Requirement: Tab Organization Is Verified
The Apple client SHALL include focused verification for Tab reorder, pin/unpin,
Move to Space, manifest persistence, context targeting, and runtime identity
preservation.

#### Scenario: Drag threshold is verified
- **WHEN** Tab row drag behavior changes
- **THEN** focused tests or script checks verify short clicks still select Tabs
  and drag begins only after the movement threshold

#### Scenario: Cross-section drag is verified
- **WHEN** drag pin/unpin behavior changes
- **THEN** verification covers Unpinned-to-Pinned snapshot creation,
  Pinned-to-Unpinned behavior, and realtime insertion preview

#### Scenario: Context target is verified
- **WHEN** Tab context menu actions change
- **THEN** verification proves actions target the clicked Tab without selecting
  it first

#### Scenario: Move to Space focus is verified
- **WHEN** Move Tab to Space behavior changes
- **THEN** verification covers current Tab follow behavior and non-current Tab
  no-follow behavior

#### Scenario: Runtime identity is verified
- **WHEN** Tab organization mutations are implemented
- **THEN** verification proves Tab IDs, pane IDs, split trees, runtime handles,
  and queued delivery state remain stable across reorder, pin/unpin, and Move
  to Space

### Requirement: Dev channel packaging has focused validation
The Apple client build/test contract SHALL include focused checks for dev
channel app metadata, embedded CLI, signing, install targets, and ownership
guards when dev channel packaging changes.

#### Scenario: Dev app layout is checked
- **WHEN** dev channel packaging implementation is ready for review
- **THEN** focused checks verify `Alan Dev.app` is built
- **AND** focused checks verify the bundle identifier is `app.alanworks.macos.dev`
- **AND** focused checks verify embedded `Contents/Resources/bin/alan-dev` exists and is executable
- **AND** focused checks verify stable `Alan.app` packaging remains unchanged

#### Scenario: Dev install ownership is checked
- **WHEN** dev channel local install checks run
- **THEN** they verify the install creates or refreshes `Alan Dev.app` and `alan-dev`
- **AND** they verify the install does not overwrite `Alan.app` or `alan`
- **AND** they verify dev uninstall does not remove stable app or stable command-line links

#### Scenario: Dev signing is checked
- **WHEN** dev channel local install produces an app bundle
- **THEN** focused checks verify the app and embedded tools are signed according to the local install signing policy
- **AND** checks report whether notarization was skipped because the dev channel is local-only

### Requirement: Channel isolation has focused verification
Changes to install-channel resolution SHALL include focused validation that
stable and dev channels resolve distinct runtime state, daemon, singleton, and
shell-control boundaries.

#### Scenario: Runtime paths are checked
- **WHEN** channel-aware path resolution changes
- **THEN** focused tests verify stable channel paths resolve under `~/.alan`
- **AND** focused tests verify dev channel paths resolve under `~/.alan-dev`
- **AND** tests cover host config, connections, credentials, agents, models, sessions, managed auth, registry, and global public skill sources

#### Scenario: Daemon endpoints are checked
- **WHEN** channel-aware daemon/client defaults change
- **THEN** focused tests verify stable and dev channels use distinct default daemon endpoints
- **AND** a missing dev config does not cause dev clients to connect to the stable daemon implicitly

#### Scenario: Shell-control namespaces are checked
- **WHEN** channel-aware shell-control paths change
- **THEN** focused tests verify stable and dev shell-control socket paths differ
- **AND** tests verify commands for one channel do not read binding files from the other channel

#### Scenario: Side-by-side smoke is checked
- **WHEN** dev channel support is considered ready for local use
- **THEN** maintainers can run an automated or documented manual smoke that keeps stable Alan installed while installing and launching Alan Dev
- **AND** the smoke verifies both apps can be identified independently by bundle id
- **AND** the smoke verifies dev session/config/auth state is written only to dev-channel paths

### Requirement: Apple quality gate includes focused macOS shell tests
The Apple client SHALL provide repeatable commands for focused macOS shell tests
covering shell model mutations, runtime service fakes, control-plane command
execution, App Intent routing, and UI smoke flows.

#### Scenario: Developer runs Apple shell tests
- **WHEN** a developer runs the documented focused Apple shell test command
- **THEN** model, fake runtime, control-plane, and intent-routing tests run without requiring the full app UI

#### Scenario: Ghostty artifacts absent
- **WHEN** Ghostty artifacts are absent and the focused test command runs
- **THEN** tests that do not require real Ghostty run normally and Ghostty integration tests are skipped or fail with documented setup instructions

#### Scenario: Ghostty artifacts present
- **WHEN** Ghostty artifacts are prepared and the integration lane is requested
- **THEN** the macOS app builds with Ghostty and runs terminal-host integration checks

### Requirement: UI smoke coverage is repeatable
The Apple client SHALL provide a repeatable UI smoke or screenshot flow for
launch, space/tab switching, split creation, command UI, pane-scoped Find
behavior, and basic terminal input when terminal runtime is available.

#### Scenario: Launch smoke
- **WHEN** the UI smoke flow starts the macOS app
- **THEN** it verifies that the default light-mode window shows the unified sidebar column, active-space tab list, bottom space switcher, terminal content area, and no persistent inspector pane or toggle

#### Scenario: Split smoke
- **WHEN** the UI smoke flow creates a split
- **THEN** it verifies that multiple panes are visible and no raw pane IDs or debug labels dominate the default UI

#### Scenario: Find smoke
- **WHEN** the UI smoke flow opens pane-scoped Find for the focused terminal pane
- **THEN** it verifies that a focused text field appears in pane chrome, result feedback is visible without raw debug labels, and dismissing Find returns focus to the owning terminal surface

### Requirement: Test fixtures share production command paths
Apple tests SHALL exercise shell mutations through the same controller command
interfaces used by menus, command UI, App Intents, and control-plane handlers.

#### Scenario: Split command fixture
- **WHEN** a test invokes split through the shared command interface
- **THEN** the resulting shell state matches the state produced by native command and control-plane paths

#### Scenario: Close command fixture
- **WHEN** a test invokes close pane through the shared command interface
- **THEN** shell state updates and runtime fake finalization are both asserted

### Requirement: Build and test documentation stays current
Apple build/test scripts, `just` commands, and `clients/apple/README.md` SHALL
document local dependency setup, focused test commands, UI smoke commands, and
Ghostty integration prerequisites.

#### Scenario: Command added
- **WHEN** a new Apple shell test or smoke command is added
- **THEN** the README and just/script references are updated in the same change

#### Scenario: Dependency changes
- **WHEN** Ghostty or App Intent test prerequisites change
- **THEN** the documented setup and failure messages are updated together

### Requirement: Content container model has focused tests
Apple client SHALL 为 v0.2 content-container model、旧状态迁移、mixed split、content-aware
command validation 和 content-keyed terminal runtime continuity 提供 focused 自动化测试或明确的人工验证记录。

#### Scenario: Old terminal state migrates
- **WHEN** 测试加载 `persist-macos-shell-workspaces` 产生的 terminal-only workspace manifest
- **THEN** shell model 迁移出 PaneSlot 和 terminal ContentInstance
- **AND** focused space、focused tab、focused PaneSlot、pin/live snapshot 和 terminal metadata projection 保持一致

#### Scenario: Legacy shell state is not restored as workspace authority
- **WHEN** 测试环境同时存在旧 `shell-state-window_main.json` 和 workspace manifest
- **THEN** app restore 使用 workspace manifest materializer
- **AND** 旧 shell-state 只作为 runtime/control-plane projection 或诊断输入存在

#### Scenario: Mixed split mutates safely
- **WHEN** 测试创建 terminal + markdown + settings 的 mixed split tab
- **THEN** split、focus、move、close 和 equalize 操作保持 split tree 有效
- **AND** terminal runtime identity 不因非 terminal PaneSlot 操作重建

#### Scenario: Non-terminal command rejection tested
- **WHEN** control-plane 测试向 markdown 或 settings PaneSlot 发送 `terminal.send_text`
- **THEN** response 使用 stable unsupported-content error
- **AND** fake terminal runtime service 没有收到 delivery

#### Scenario: Terminal content identity survives movement
- **WHEN** 测试将 terminal ContentInstance 所在 PaneSlot 移动到另一个 tab 或重新 attach 视图
- **THEN** terminal runtime handle、scrollback、metadata 和 pending delivery 仍绑定到同一个 `content_id`

#### Scenario: Retired unpinned tab finalizes terminal content
- **WHEN** workspace lifecycle pruning retires an inactive unpinned Tab that contains terminal ContentInstances
- **THEN** focused tests verify those terminal runtimes are finalized through the runtime service
- **AND** retired PaneSlots and terminal ContentInstances cannot receive later `terminal.send_text` delivery

#### Scenario: Content rendering registry verified
- **WHEN** renderer registry 收到 terminal、markdown 和 settings content descriptor
- **THEN** 测试或 review checklist 确认每个 kind 路由到对应 renderer 或 bounded unavailable surface

#### Scenario: Visual evidence covers mixed content
- **WHEN** content-container UI implementation 标记完成
- **THEN** maintainers 可以检查 running-app screenshot 或记录，确认 light-mode shell 中存在混合 content tab/split，且 sidebar、toolbar、pane title bar 和 terminal-first chrome 保持一致

### Requirement: Single alan binary distribution is verified
The Apple client and release packaging checks SHALL verify that terminal
distribution uses one command-line executable, `alan`, and SHALL reject
reintroducing `alan-tui` as an embedded, signed, linked, installed, or launched
binary.

#### Scenario: Release app layout is checked
- **WHEN** release packaging implementation is ready for review
- **THEN** focused checks verify `Alan.app` contains the app executable and the
  embedded command-line `alan` executable required by the distribution contract
- **AND** focused checks fail if `Alan.app` contains an embedded
  `Contents/Resources/bin/alan-tui` executable

#### Scenario: Signatures are checked
- **WHEN** release signing checks inspect embedded command-line tools
- **THEN** they verify the embedded `alan` binary is signed according to the
  release signing contract
- **AND** they do not require or accept a separately signed `alan-tui` binary as
  part of the release package

#### Scenario: Command-line links are checked
- **WHEN** direct app install scripts or future Homebrew cask metadata are
  inspected
- **THEN** they link or install only `alan` from the app bundle
- **AND** they fail if they link, install, or document `alan-tui`

### Requirement: Legacy TypeScript TUI build paths are blocked
The Apple client build/test contract SHALL include focused validation that
prevents production packaging, install, and shell launch flows from depending on
the deleted TypeScript/Bun/Ink TUI.

#### Scenario: Bun TUI build is reintroduced
- **WHEN** release packaging or local install scripts are inspected
- **THEN** focused checks fail if they build, bundle, sign, or launch
  `clients/tui`, Bun, Ink, or a standalone TypeScript TUI artifact

#### Scenario: TUI path override is reintroduced
- **WHEN** command-line launch or app shell scripts are inspected
- **THEN** focused checks fail if production code uses `ALAN_TUI_PATH` as a
  fallback path for the terminal UI

#### Scenario: Legacy TUI dependency remains
- **WHEN** package metadata and install scripts are inspected after migration
- **THEN** no required production build or install step depends on the deleted
  TypeScript TUI package

### Requirement: Apple shell launches bare alan
The Apple shell SHALL launch the command-line terminal experience through bare
`alan` and SHALL NOT launch `alan chat`, `alan ask`, or `alan-tui` for the
default alan terminal tab.

#### Scenario: Default alan tab command is inspected
- **WHEN** shell contract checks inspect the default alan terminal launch command
- **THEN** the command resolves to bare `alan`
- **AND** it does not include `chat`, `ask`, or `alan-tui`

#### Scenario: Legacy commands are reintroduced
- **WHEN** Apple shell scripts, control paths, or documentation reintroduce
  `alan chat`, `alan ask`, or `alan-tui` as the default terminal launch path
- **THEN** focused shell contract checks fail with a message pointing to the
  single-binary Rust TUI contract

### Requirement: Auto-update packaging has focused validation
The Apple client build/test contract SHALL provide focused validation for
Sparkle integration, appcast generation, release archive trust metadata, and
direct-install update behavior when macOS auto-update support changes.

#### Scenario: Sparkle integration is checked
- **WHEN** auto-update implementation is ready for review
- **THEN** focused checks verify the release app includes the Sparkle framework and helper code required for updates
- **AND** focused checks verify Sparkle public-key and feed URL metadata are present in the release app bundle
- **AND** focused checks verify Sparkle nested code and the final app bundle are Developer ID signed

#### Scenario: Appcast artifact is checked
- **WHEN** a release appcast is generated
- **THEN** focused checks verify `appcast.xml` references the expected GitHub Release zip asset
- **AND** focused checks verify the appcast includes Sparkle EdDSA signature metadata for the update archive
- **AND** focused checks verify the appcast version and short version match the release package metadata

#### Scenario: Cloudflare appcast headers are checked
- **WHEN** `https://alanworks.app/appcast.xml` is deployed for stable updates
- **THEN** focused checks verify the response is served as XML
- **AND** focused checks verify cache headers do not create a long-lived stale feed

#### Scenario: Direct app update smoke is checked
- **WHEN** auto-update support is considered release-ready
- **THEN** verification installs or launches an older signed and notarized `Alan.app`
- **AND** verification confirms the appcast short version and build match the newer app bundle
- **AND** verification confirms Sparkle detects the newer appcast item
- **AND** verification confirms the update downloads, verifies, installs, and relaunches into the newer version

#### Scenario: Homebrew path is checked
- **WHEN** update behavior is tested for a Homebrew-managed install
- **THEN** focused checks verify Alan does not let Sparkle replace the Homebrew-managed app bundle
- **AND** focused checks verify the user-facing update path points to Homebrew instead
