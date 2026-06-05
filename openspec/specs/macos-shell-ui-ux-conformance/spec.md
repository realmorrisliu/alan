# macos-shell-ui-ux-conformance Specification

## Purpose
Define the default native macOS shell UI contract: Arc-like space/tab
organization, terminal-first layout, native light-mode material treatment,
restrained toolbar behavior, pane-scoped terminal controls, and progressive
disclosure that keeps debug surfaces out of the default shell.
## Requirements
### Requirement: Visual system follows native material guidance
The default macOS shell SHALL use a light-mode-first native material visual
system that feels calm, precise, and terminal-oriented. It SHALL avoid
card-heavy dashboard composition, decorative gradients, hard-coded dominant
theme panels, and ornamental controls.

#### Scenario: Material sidebar
- **WHEN** the app window is visible in the default light appearance
- **THEN** the unified sidebar column, top Space slider, active-Space tab list,
  and compact sidebar controls use material-backed surfaces, subtle separators
  where useful, and restrained selection states rather than an opaque themed
  sidebar panel, separate Space rail, or bottom Space switcher

#### Scenario: Stable compact controls
- **WHEN** the user hovers, selects, inserts, closes, creates, or switches tabs
  and Spaces
- **THEN** rows, icon controls, dots, counters, and status marks keep stable
  dimensions and do not resize the sidebar or terminal content
- **AND** the top Space slider aligns its visible controls to the sidebar edge
  inset so Space slider targets and tab rows share one optical column
- **AND** the titlebar New Space button directly creates a standard new Space
  instead of opening a menu of Space variants
- **AND** the titlebar New Space button is right-aligned within the sidebar
  titlebar instead of sitting immediately after the pin/unpin and appearance
  controls

#### Scenario: Space slider replaces header and dock
- **WHEN** the default sidebar displays Space navigation
- **THEN** the selected Space is represented by its title in the top Space
  slider, non-selected Spaces use the representation required by the active
  Space slider density tier, and no Space icon is shown in the slider
- **AND** alan does not show a separate bottom Space dock in the default sidebar
- **AND** the top Space slider remains a fixed sidebar-level control while
  Space tab content pages move beneath it during Space paging gestures

#### Scenario: Space profile uses context menu disclosure
- **WHEN** the default sidebar displays the selected Space
- **THEN** alan does not show an always-visible terminal profile selector in the
  Space slider or default sidebar
- **AND WHEN** the user opens a Space context menu from the selected Space title
  or a non-selected Space slider target
- **THEN** terminal profile selection is available as a Space-level context menu
  action

#### Scenario: New Tab belongs to ordinary tab flow
- **WHEN** the active Space contains pinned tabs
- **THEN** alan shows pinned tabs first, then a subtle divider, then the New Tab
  row, then unpinned tabs
- **AND WHEN** the active Space contains no pinned tabs
- **THEN** alan shows the New Tab row before unpinned tabs without a pinned-tab
  divider
- **AND** the New Tab row creates a normal unpinned terminal tab in the current
  Space

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL adapt its visual density to the
number of Spaces and SHALL support deliberate preview-first scrub navigation
without making the default sidebar visually noisy or unstable.

#### Scenario: Low-density Space slider shows full titles
- **WHEN** the default sidebar has 1 to 3 Spaces
- **THEN** the Space slider shows every Space as a named, single-line
  Safari-like tab or pill control
- **AND** the selected Space has the strongest material and text treatment
- **AND** inactive Spaces remain readable without using Space icons

#### Scenario: Mid-density Space slider shows active and short titles
- **WHEN** the default sidebar has 4 to 6 Spaces
- **THEN** the Space slider shows the selected Space with its full title
- **AND** every inactive Space remains visible as a compact short-title control
- **AND** inactive Space titles truncate on one line before they resize the
  sidebar, wrap text, or overlap adjacent controls

#### Scenario: High-density Space slider uses active title and indicators
- **WHEN** the default sidebar has 7 to 9 Spaces
- **THEN** the Space slider shows the selected Space as a title control
- **AND** inactive Spaces render as compact indicators in the default idle state
- **AND** hovered or scrub-focused inactive indicators can expand into short
  title controls without changing slider height or sidebar width

#### Scenario: Space count is capped at nine
- **WHEN** the user attempts to create Spaces from the default macOS shell
- **THEN** alan allows at most 9 Spaces in the Space slider model
- **AND** creation affordances do not produce a tenth visible Space in the
  default sidebar

#### Scenario: Hover previews without switching
- **WHEN** the pointer hovers a non-selected Space in the slider
- **THEN** alan locally highlights or expands that Space according to the active
  density tier
- **AND** alan does not switch the selected Space, move the tab pager, or change
  focused terminal content merely because of hover

#### Scenario: Click switching remains immediate
- **WHEN** the user clicks a non-selected Space in the slider
- **THEN** alan immediately selects that Space
- **AND WHEN** the user clicks the selected Space
- **THEN** alan keeps the current selection unchanged

#### Scenario: Scrub previews before commit
- **WHEN** the user press-drags horizontally on the Space slider or sends clear
  horizontal wheel or trackpad input while hovering the slider
- **THEN** alan enters a scrub preview state with a focused target Space
- **AND** alan distinguishes the scrub focus from the currently selected Space
  when they differ
- **AND** alan does not commit Space selection until drag release or a short
  dwell after wheel or trackpad input stops

#### Scenario: Scrub uses lightweight cover-flow motion
- **WHEN** reduced motion is disabled and Space scrub is active
- **THEN** the scrub-focused Space is emphasized as the largest title control
- **AND** nearby Spaces scale or fade by distance to communicate direction
- **AND** the effect remains bounded inside the sidebar Space slider rather than
  introducing a large carousel or decorative overlay

#### Scenario: Vertical scrolling is preserved
- **WHEN** wheel or trackpad input over the Space slider is vertical or
  ambiguous between horizontal and vertical intent
- **THEN** alan does not enter Space scrub
- **AND** the tab list can continue receiving vertical scrolling behavior

#### Scenario: Space context menus remain available
- **WHEN** the user right-clicks or opens the context menu for a selected,
  hovered, or scrub-focused Space target
- **THEN** alan opens the context menu for that Space
- **AND** any active scrub preview is canceled before the context menu action
  changes Space-level settings

#### Scenario: Reduced motion preserves the state model
- **WHEN** reduced motion is enabled
- **THEN** hover and scrub use bounded highlight, opacity, and width changes
  instead of cover-flow scale, springy movement, or perspective-like motion
- **AND** click, hover, scrub preview, commit, cancel, and keyboard behavior
  remain equivalent

#### Scenario: Keyboard and accessibility navigation remain explicit
- **WHEN** keyboard focus or VoiceOver reaches the Space slider
- **THEN** each Space is exposed as a distinct actionable target with its title,
  selected state, and tab count
- **AND** left and right keyboard navigation can move preview focus without
  immediately switching Spaces
- **AND** Enter commits the focused Space and Escape cancels preview focus

### Requirement: Collapsed sidebar uses a lightweight floating panel
When the sidebar is collapsed, the macOS shell SHALL reveal navigation through a
small floating material panel triggered by intentional edge or titlebar-control
hover, while keeping the terminal workspace stable.

#### Scenario: Narrow reveal target
- **WHEN** the sidebar is collapsed and the pointer approaches the left edge
- **THEN** alan uses a narrow edge hot zone to reveal the floating sidebar panel rather than a full titlebar or header-width hover region

#### Scenario: Floating panel hover retention
- **WHEN** the pointer moves from the edge hot zone onto the floating sidebar panel or collapsed titlebar controls
- **THEN** the floating panel remains revealed until the pointer leaves those related surfaces

#### Scenario: Window-edge hover retention
- **WHEN** the sidebar is collapsed, the floating panel is revealed, and the pointer crosses from the edge hot zone or floating panel into the left window resize frame
- **THEN** alan treats that pointer position as part of the collapsed-sidebar reveal neighborhood and keeps the floating panel revealed
- **AND** alan does not schedule a hide merely because AppKit has switched the cursor or hit-test state to a window-resize affordance
- **AND** native window resizing remains available if the user presses and drags in the resize frame

#### Scenario: Visible-frame zoom edge retention
- **WHEN** the shell window has been double-click zoomed to the current screen's visible work area and its left edge is flush with the usable screen boundary
- **AND** the sidebar is collapsed and revealed from the left edge
- **THEN** moving the pointer along the left edge or through the resize-cursor strip does not cause the floating sidebar to auto-hide while the pointer remains in the window-level reveal neighborhood

#### Scenario: Floating panel owns traffic lights
- **WHEN** the sidebar is collapsed and the floating panel is hidden
- **THEN** the standard macOS traffic-light controls are hidden with the sidebar surface instead of remaining on the bare window corner
- **AND WHEN** the floating sidebar panel is revealed
- **THEN** the standard macOS traffic-light controls reappear on that floating sidebar surface without appearing ahead of the panel reveal timing, jumping from the non-floating corner, or changing terminal workspace geometry

#### Scenario: Floating panel motion
- **WHEN** reduced motion is disabled
- **THEN** the floating sidebar panel enters with a short spring-like leading-edge reveal and exits with a faster low-emphasis hide animation
- **AND** the standard macOS traffic-light controls and lightweight sidebar titlebar controls move with the visible floating surface instead of snapping after the panel has moved

#### Scenario: Reduced motion respected
- **WHEN** reduced motion is enabled
- **THEN** collapsed-sidebar reveal and hide behavior avoids springy movement while preserving the same hover targets and visibility state

#### Scenario: Workspace stability
- **WHEN** the floating sidebar panel appears or disappears
- **THEN** terminal content, split geometry, and window size remain stable instead of being resized by the transient sidebar surface

#### Scenario: No dashboard treatment
- **WHEN** the user views the default shell
- **THEN** the UI does not present page-like sections, nested cards, large explanatory panels, or marketing-style hero composition

### Requirement: Terminal content is the center of gravity
The main content region SHALL make the active terminal canvas visually dominant
and SHALL avoid nested decorative panels around the terminal in the default
single-pane state.

#### Scenario: Single-pane tab
- **WHEN** a tab contains one pane
- **THEN** the terminal appears nearly full-bleed within the content region and
  does not show a pane selector strip

#### Scenario: Split-pane tab
- **WHEN** a tab contains multiple panes
- **THEN** pane chrome stays lightweight and focus is conveyed by subtle
  selection treatment rather than explicit engineering labels

#### Scenario: alan as optional capability
- **WHEN** a terminal tab is active and no alan-specific surface has been opened
- **THEN** alan appears as an optional command or attachment capability layered
  onto the terminal workflow, not as the structural center of the window

### Requirement: Default UI hides implementation jargon
The default macOS UI SHALL avoid exposing raw pane IDs, `tab_id`, binding,
runtime phases, `window attached`, `title updated`, and other implementation
terms outside explicit debug surfaces. It SHALL also avoid obsolete product
labels such as `AlanNative` or `Alan Shell` in visible app chrome.

#### Scenario: Normal terminal workflow
- **WHEN** a user creates, selects, splits, or closes tabs and panes
- **THEN** visible copy uses product terms such as Space, Tab, Split, Go to or
  Command, Open in alan, and Ask alan

#### Scenario: Command input routing states
- **WHEN** the command input opens, submits a supported typed command, or
  reports an unresolved typed command
- **THEN** the input and inline status use user-facing names where available and
  do not expose raw pane IDs, routing internals, or debug identifiers as the
  primary label
- **AND** alan does not open default tabs, panes, actions, routing-candidate,
  attention, best-match, or command-row sections below the field

#### Scenario: Debug surfaces
- **WHEN** implementation details are needed
- **THEN** they remain in explicit debug-only surfaces, logs, scripts, or
  snapshots rather than default shell chrome

### Requirement: Toolbar is native and restrained
The macOS toolbar/titlebar SHALL feel like native window chrome and contain only
the current tab title/context, one command entry point, and a small number of
frequent actions.

#### Scenario: Toolbar default state
- **WHEN** no urgent attention item exists
- **THEN** the toolbar does not show attention as a large standalone primary control

#### Scenario: Command entry
- **WHEN** the user invokes the command UI
- **THEN** the entry point is labeled and organized as `Go to or Command...`
- **AND WHEN** the floating Ask alan command input is presented
- **THEN** the input surface uses the system Liquid Glass effect for its primary material instead of relying on a custom blur, opaque fill, or gradient-only imitation
- **AND** the Liquid Glass material follows the app's current light or dark appearance instead of assuming the active terminal theme is dark
- **AND** the active terminal theme only contributes sampled background color through the system material and MUST NOT switch the command input's light or dark foreground palette
- **AND** foreground text and controls render above the Liquid Glass layer using system foreground hierarchy so the glass material does not blur or wash out typed content
- **AND** alan keeps the Liquid Glass surface mounted across hidden and visible states, disables material insertion animation, and uses an opacity-only fade instead of moving the input from an edge
- **AND** alan uses a transparent click-away layer rather than a visible dimming scrim behind the Liquid Glass surface so the material does not flash between undimmed and dimmed sampled backgrounds
- **AND** pressing `Command-P` while the input is already open dismisses it and returns keyboard focus to the previously focused terminal pane when available

#### Scenario: Empty titlebar zoom
- **WHEN** a user double-clicks an empty, non-control area of the hidden-titlebar chrome
- **THEN** alan toggles the window between its previous frame and the current screen's visible work area while leaving the system traffic-light buttons, including the green button, on their normal macOS behavior
- **AND** empty sidebar or floating-sidebar chrome in the traffic-light/titlebar-control band participates in double-click zoom while the actual traffic-light buttons, lightweight titlebar buttons, and terminal pane titlebar controls remain clickable

#### Scenario: Native fullscreen chrome
- **WHEN** the hidden-titlebar shell window enters native macOS fullscreen and the system takes over or hides the traffic-light controls
- **THEN** alan moves its lightweight titlebar controls to the leading edge without reserving traffic-light space
- **AND WHEN** the window is actively live-resized
- **THEN** alan continuously resynchronizes the standard traffic-light controls during the resize interaction rather than only correcting the final resting position
- **AND WHEN** the window exits native fullscreen or finishes resizing
- **THEN** alan keeps the standard traffic-light controls at their intended inset and returns its titlebar controls to the post-traffic-light position

### Requirement: Default shell does not expose inspector chrome
The default macOS shell SHALL not include a persistent right-side inspector,
inspector toggle, or inspector command surface.

#### Scenario: Default shell opened
- **WHEN** the user opens the macOS shell
- **THEN** no inspector pane, inspector toggle, or inspector-specific command appears in the default UI

#### Scenario: Diagnostics needed
- **WHEN** maintainers need runtime diagnostics
- **THEN** diagnostics remain available through shell snapshots, logs, scripts, tests, or an explicit future debug surface rather than a default inspector

### Requirement: UI conformance is verified visually
Mac shell UI changes SHALL be reviewed against the documented UI contract before
the UI conformance tasks are marked complete.

#### Scenario: Default screenshot review
- **WHEN** a UI conformance implementation pass is ready for review
- **THEN** maintainers can inspect a running-app screenshot of the default light-mode window showing the top Space slider, active-space tab list, terminal-first content area, and no inspector surface

#### Scenario: Removed-inspector review
- **WHEN** inspector-removal UI tasks are marked complete
- **THEN** maintainers can inspect screenshots or recorded notes confirming the default shell has no right-side inspector and no inspector toggle

### Requirement: Terminal overlays use user-facing language
The macOS terminal UI SHALL present child-exit, renderer failure, readonly,
input-not-ready, and clipboard states with concise terminal-user language in
the canvas area, while raw runtime details remain debug-only.

#### Scenario: Renderer failure visible
- **WHEN** a focused terminal pane cannot render
- **THEN** the default UI explains that the terminal cannot draw and offers an actionable next step without showing raw Ghostty callback names or pane IDs

#### Scenario: Child exit visible
- **WHEN** a terminal child process exits
- **THEN** the pane shows a compact terminal exit state rather than debug event names

#### Scenario: Debug diagnostics inspected
- **WHEN** maintainers inspect explicit debug diagnostics
- **THEN** renderer diagnostics, surface identifiers, input mode details, and raw event payloads use debug framing outside the default shell

### Requirement: Terminal search does not displace workspace structure
Terminal search UI SHALL be compact, pane scoped, and layered over the terminal
workflow without turning the shell into a dashboard or page layout.

#### Scenario: Search opens
- **WHEN** the user invokes terminal search
- **THEN** the search control appears as a compact terminal tool for the focused pane and the sidebar, toolbar, and split layout keep stable dimensions

#### Scenario: Search closes
- **WHEN** the user dismisses terminal search
- **THEN** keyboard focus returns to the terminal pane that owned the search interaction

### Requirement: Native Find bar owns terminal search UI
Terminal search SHALL render through a compact pane-scoped Find bar rather than
the passive terminal overlay card, and query edits SHALL be routed to the
owning terminal surface search controller.

#### Scenario: Find opens
- **WHEN** the user invokes `Command-F` for a focused pane
- **THEN** alan shows a compact Find bar for that pane, focuses the query field, and does not send printable query text to the terminal application

#### Scenario: Find navigates
- **WHEN** the Find bar owns an active query
- **THEN** Return, `Command-G`, and Shift-`Command-G` navigate matches through the pane's search owner

#### Scenario: Find dismisses
- **WHEN** the user presses Escape or clicks the close control
- **THEN** alan dismisses the Find interaction and returns focus to the owning terminal pane

### Requirement: Copy paste and search surfaces are native and pane scoped
Copy, paste, and search command UI SHALL feel native, target the focused PaneSlot,
and avoid displacing the sidebar, toolbar, or split layout.

#### Scenario: Search opens
- **WHEN** the user invokes terminal search
- **THEN** the search UI appears as a compact pane-scoped terminal tool

#### Scenario: Copy paste available
- **WHEN** the focused PaneSlot mounts terminal content that can copy or paste
- **THEN** native menu and keyboard commands target that terminal content without exposing debug routing details

### Requirement: Terminal panes have unambiguous hit-testing boundaries
The macOS shell UI SHALL keep terminal-rendering surfaces from intercepting
mouse events that must be handled by the terminal host, while preserving
explicit SwiftUI/AppKit controls outside the terminal pane.

#### Scenario: Rendering canvas is clicked
- **WHEN** a user clicks the Ghostty or fallback rendering canvas inside a terminal pane
- **THEN** AppKit hit-testing delivers the event to the terminal host rather than treating the canvas as a separate interactive owner

#### Scenario: Passive terminal overlay is visible
- **WHEN** a non-interactive terminal placeholder or diagnostic overlay is visible over the terminal canvas
- **THEN** the overlay does not prevent the terminal host from receiving pane activation clicks

#### Scenario: Pane selector button is clicked
- **WHEN** a user clicks an explicit pane selector button outside the terminal canvas
- **THEN** that SwiftUI control handles selection through its own action without routing the click through the terminal host

### Requirement: Window dragging excludes terminal panes
The macOS shell UI SHALL allow non-interactive window background regions to drag
the hidden-titlebar shell window and SHALL prevent terminal-pane interactions
from initiating window dragging.

#### Scenario: Background chrome is dragged
- **WHEN** a user drags a non-interactive shell background area outside terminal panes and controls
- **THEN** the window moves according to the native movable-background behavior

#### Scenario: Terminal pane is dragged
- **WHEN** a user drags inside a terminal pane
- **THEN** the drag is handled as terminal input or terminal selection and does not move the window

### Requirement: Split UI is terminal first
Split-pane UI SHALL use lightweight dividers, subtle focus treatment, and
stable geometry so the terminal remains the visual center rather than becoming a
card grid or debug layout.

#### Scenario: Multiple panes visible
- **WHEN** a tab contains multiple visible terminal panes
- **THEN** dividers and focus treatment are compact and do not show raw pane IDs, runtime phases, or redundant labels by default

#### Scenario: Split panes share one terminal surface
- **WHEN** a tab contains adjacent visible terminal panes
- **THEN** panes are rendered inside one continuous terminal surface whose outer four corners are rounded, with no per-pane rounded cards, shadows, bottom pane tab strip, or fixed gaps; only a subtle low-contrast beveled split seam separates neighboring panes

#### Scenario: Divider hover
- **WHEN** the user hovers or drags a split divider
- **THEN** the divider provides a clear native resize affordance without resizing unrelated sidebar or toolbar elements

#### Scenario: Inactive split pane
- **WHEN** a split pane is not the active terminal pane
- **THEN** alan may apply a preference-backed lightweight dim treatment that preserves terminal readability and pointer input while making the active pane and split boundary easier to scan

### Requirement: Zoom affordances stay compact
Split zoom UI SHALL make the zoomed state and escape path clear without adding a
persistent pane-management toolbar.

#### Scenario: Pane zoomed
- **WHEN** a PaneSlot is zoomed
- **THEN** the UI provides a compact way to unzoom while keeping the terminal content dominant

#### Scenario: Toolbar remains restrained
- **WHEN** zoom is available for a split pane
- **THEN** the default toolbar does not add a dense split-control strip

### Requirement: Movement affordances protect terminal interaction
Pane movement UI SHALL avoid ambiguous gestures inside terminal content and keep
terminal text selection reliable.

#### Scenario: Movement command shown
- **WHEN** the command UI or context menu offers pane movement
- **THEN** the label describes the destination or action in user-facing terms without raw pane IDs

#### Scenario: Drag affordance visible
- **WHEN** drag/drop pane movement is enabled
- **THEN** the movement affordance is visually distinct from terminal text selection regions

### Requirement: Terminal panes expose narrow title bars
Each visible macOS terminal pane SHALL include a compact title bar at the top of
the pane that identifies the terminal and provides a pane-scoped close
affordance while keeping terminal content visually dominant. The title bar SHALL
read as part of the terminal surface rather than as a separate selected chrome
overlay.

#### Scenario: Single pane title visible
- **WHEN** a terminal tab contains one visible pane
- **THEN** the pane shows a narrow title bar above the terminal canvas with a user-facing terminal title and one slim close button
- **AND** the title bar uses the same terminal-surface background as the canvas rather than a selected/unselected material wash above it

#### Scenario: Split pane titles visible
- **WHEN** a terminal tab contains multiple visible panes
- **THEN** every pane leaf shows its own title bar and close button without adding a pane selector strip, card grid, or debug labels
- **AND** title-bar backgrounds do not create per-pane cards, opaque overlays, or separate toolbar bands above each terminal pane

#### Scenario: Focused title remains readable
- **WHEN** a pane becomes the focused terminal pane
- **THEN** the pane title remains visible as text with sufficient foreground contrast against the terminal surface background
- **AND** focused state does not hide the title, blend it into the title-bar background, or replace it with an icon-only representation

#### Scenario: Long title fits
- **WHEN** a pane title is long or changes while the pane is visible
- **THEN** the title truncates within a stable fixed-height title bar without resizing split dividers, sidebar rows, toolbar content, or sibling panes

#### Scenario: Narrow title bar degrades predictably
- **WHEN** a pane title bar does not have enough width to show all detail
- **THEN** lower-priority accessories degrade from text plus icon to icon-only or hidden before the title text or close affordance disappear
- **AND** the title remains text with truncation rather than degrading to icon-only content

### Requirement: Pane title bars consume terminal metadata
Pane title bars SHALL consume the current terminal title already projected into
pane metadata, and SHALL use existing user-facing fallback labels only when the
terminal title is unavailable. Pane title-bar detail SHALL be presented in
left-to-right semantic priority order using fit-content item widths where space
allows.

#### Scenario: Terminal title exists
- **WHEN** a pane has a non-empty `viewport.title`
- **THEN** the title bar shows the normalized terminal title rather than raw pane IDs, cwd-first labels, runtime phases, or debug event text

#### Scenario: Terminal title missing
- **WHEN** a pane has no usable terminal title
- **THEN** the title bar falls back to cwd leaf, working-directory name, launch target, process name, or `Terminal` using user-facing copy

#### Scenario: Debug terms suppressed
- **WHEN** terminal metadata contains implementation-oriented summaries such as `title updated`, `window attached`, or raw runtime state
- **THEN** the title bar does not expose those terms outside explicit developer/debug-only surfaces

#### Scenario: Metadata stays in title chrome
- **WHEN** terminal status, branch, attention, or alan binding metadata is useful in the default pane UI
- **THEN** alan presents it as lightweight pane-title-bar accessories rather than as a persistent bottom status strip below the terminal canvas

#### Scenario: Detail order is semantic
- **WHEN** a pane title bar has title, activity, status, cwd or worktree, branch, process, alan state, and close detail available
- **THEN** alan orders visible content from left to right as title, activity/status, cwd or worktree, branch, process or alan state, and close
- **AND** each visible item uses fit-content width rather than reserving a fixed-width column for every accessory

#### Scenario: Detail fallback preserves priority
- **WHEN** available title-bar width cannot fit all detail labels
- **THEN** activity and status detail outrank cwd, worktree, branch, process, and alan detail
- **AND** cwd, worktree, branch, process, and alan detail can collapse to icon-only or hide before the title text collapses

### Requirement: Pane close button targets its pane
The pane title bar close button SHALL close the pane represented by that title
bar through the shared shell controller mutation path.

#### Scenario: Inactive split pane closed
- **WHEN** a user clicks the close button on a non-selected visible split pane
- **THEN** alan closes that pane, repairs the split tree, and keeps the remaining pane runtimes alive without closing a different selected pane

#### Scenario: Single pane tab closed
- **WHEN** a user clicks the close button for the only pane in a tab and other tabs remain
- **THEN** alan applies the existing tab-close semantics for that tab and focuses a remaining terminal pane

#### Scenario: Last remaining pane protected
- **WHEN** a close button targets the only pane in the only remaining tab
- **THEN** alan keeps the shell state valid and does not remove the final workspace surface

### Requirement: Pane title bars preserve terminal input ownership
Pane title bars SHALL own only their explicit title and button controls, and
SHALL not intercept terminal input, selection, mouse reporting, scrollback, or
renderer hit-testing inside the terminal canvas.

#### Scenario: Terminal canvas clicked below title bar
- **WHEN** a user clicks, drags, scrolls, or right-clicks inside the terminal canvas below a pane title bar
- **THEN** the terminal host receives the event according to the terminal event ownership contract

#### Scenario: Close button clicked
- **WHEN** a user clicks the close button in the pane title bar
- **THEN** the button handles the pane close action without routing that click through terminal text input

#### Scenario: Title area clicked
- **WHEN** a user clicks the non-button title area
- **THEN** alan may focus the pane, but it does not send text, mouse reports, or scroll events to the terminal application

### Requirement: Corner radii are restrained and tokenized
The default alan macOS shell UI SHALL use a small role-based corner-radius scale
for rounded rectangular surfaces and controls. It SHALL avoid large ad hoc
radii and capsule-heavy default chrome.

#### Scenario: Radius scale applied
- **WHEN** the active macOS shell renders sidebar rows, command rows, pane title bars, terminal surrounds, inline panels, or overlay surfaces
- **THEN** those rounded rectangular elements use the alan shell radius scale rather than one-off numeric radii

#### Scenario: Default shell avoids large radii
- **WHEN** a default shell surface is visible in normal light-mode use
- **THEN** rounded rectangular chrome does not use radii larger than the overlay radius unless a specific exception is documented in the UI contract

#### Scenario: Capsule use is limited
- **WHEN** the default shell shows text chips, keycap hints, metadata chips, command badges, sidebar controls, or pane title controls
- **THEN** those controls use restrained rounded rectangles rather than `Capsule` shapes unless the component is explicitly defined as a semantic pill

#### Scenario: True circles remain semantic
- **WHEN** the shell shows attention dots, status indicators, traffic-light-like indicators, or intentionally round icon-only controls
- **THEN** those elements may remain circular because the circle communicates state or system-like control behavior

#### Scenario: Terminal surface remains precise
- **WHEN** a single pane or split-pane tab is visible
- **THEN** terminal panes keep a shared continuous terminal surround with smaller outer corners and no per-pane rounded card treatment

### Requirement: Radius normalization preserves shell hierarchy
Radius normalization SHALL make alan feel calmer and more precise without
turning the UI into a flat grid or weakening control affordances.

#### Scenario: Sidebar remains skimmable
- **WHEN** sidebar spaces, tabs, command entry, and creation controls are visible
- **THEN** smaller radii preserve row scanning, hover states, selected states, and stable dimensions

#### Scenario: Command input remains readable
- **WHEN** the command input is open
- **THEN** the floating input surface, text field, close control, and inline unresolved state use distinct but restrained radii so hierarchy is visible without turning the command input into a large palette or decorative card

#### Scenario: Overlays remain secondary
- **WHEN** the command input, Find bar, or another remaining default-shell overlay is visible
- **THEN** that surface uses restrained radii and does not read as a large decorative card competing with the terminal

### Requirement: Command UI owns navigation and shell actions
The default command entry SHALL provide a typed `Go to or Command...` input for
supported shell workspace actions and routing targets. It SHALL execute
resolved typed submissions through the shared shell controller mutation path
where the action is shared, and it SHALL avoid default visible candidate rows or
multi-section command chrome.

#### Scenario: Command input opens
- **WHEN** the user invokes `Command-P` or activates `Go to or Command...`
- **THEN** alan focuses a single floating command input field instead of presenting default tabs, panes, actions, routing-candidate, attention, or best-match lists

#### Scenario: Command input executes supported action
- **WHEN** the user submits typed text that alan can resolve to a supported workspace action or routing target
- **THEN** alan runs the same shell controller mutation used by menu and keyboard paths where that action is shared
- **AND** the command input dismisses and restores focus to the previously focused terminal pane when available

#### Scenario: Command input reports unresolved text
- **WHEN** the user submits typed text that alan cannot resolve
- **THEN** alan leaves the command input open and communicates the unresolved state inline without opening candidate rows or exposing raw debug identifiers

### Requirement: Toolbar stays restrained during split interactions
Advanced split, focus, resize, equalize, close, and pane lift affordances SHALL
not turn the toolbar into a dense control strip.

#### Scenario: Multiple panes visible
- **WHEN** a tab contains multiple panes
- **THEN** the default toolbar remains focused on current tab context, command entry, and frequent actions

#### Scenario: Pane lift available
- **WHEN** pane lift is available through command UI or another explicit non-terminal affordance
- **THEN** the default toolbar does not add a persistent pane-management strip

### Requirement: Sidebar matches single-column space/tab navigation
The default macOS sidebar SHALL remain a single vertical navigation column that
aligns cleanly around the macOS traffic-light area, with a restrained initial
width around 264 pt. Spaces SHALL be switched through compact top Space slider
targets and horizontal sidebar swipe gestures, while tabs for the active space
remain the primary sidebar list.
The sidebar surface SHALL read as a unified tinted macOS material stack, with
visual effect material, cool translucent wash, control alpha, and row shadows
working together rather than as an opaque white panel with independent cards.
Horizontal sidebar swipe SHALL feel like direct manipulation: content tracks the
gesture inside the sidebar, previews the adjacent space there, and commits or
cancels on release rather than acting as a threshold-only trigger. The workspace
surface SHALL remain visually stable during the sidebar swipe and update only
after the switch commits. The sidebar SHALL be self-explaining through spatial
structure, iconography, selection treatment, hover/focus affordances, and
accessibility labels rather than persistent instructional copy.

#### Scenario: Default sidebar reading order
- **WHEN** a user opens the macOS app
- **THEN** the sidebar reads as a top Space slider and active-space tab list in one vertical column rather than as unrelated dashboard sections or a two-column sidebar
- **AND** the sidebar surface has a cool material tint that remains coherent across empty space, controls, rows, and the top Space slider

#### Scenario: Space selection
- **WHEN** a user selects a Space in the top Space slider
- **THEN** the tab list updates to show only tabs belonging to that active space

#### Scenario: Sidebar swipe switches spaces
- **WHEN** a user performs a clear horizontal swipe gesture inside the sidebar
- **THEN** alan previews the previous or next space with gesture-tracked motion across the active-space tab list while the top Space slider remains fixed
- **AND** the preview is rendered from horizontal finger translation across the full sidebar page width rather than from threshold-derived progress
- **AND** the moving pages do not expose static left or right padding gaps
- **AND** the workspace terminal surface remains on the current space during the drag
- **AND** alan commits to the previewed space only after the user releases past a distance or velocity threshold
- **AND** a fast horizontal flick can commit from release velocity even when the visible drag distance is short
- **AND** the workspace terminal surface updates through the committed shell selection after the transition settles
- **AND** alan cancels back to the original space when the release does not meet the commit threshold
- **AND** once horizontal intent is locked, vertical movement is not applied to the tab list even if the fingers move upward or downward before release
- **AND** once vertical intent is locked, vertical tab-list scrolling remains native and is not consumed by the horizontal space pager

#### Scenario: Space swipe reaches an edge
- **WHEN** a user swipes beyond the first or last space
- **THEN** the sidebar uses a resisted edge motion instead of wrapping or abruptly changing selection

#### Scenario: Reduced motion space swipe
- **WHEN** reduced motion is enabled
- **THEN** alan may reduce the transition to a shorter fade or lower-distance movement while preserving release-based commit and cancel semantics

#### Scenario: Separate creation affordances
- **WHEN** a user creates a new space or a new tab
- **THEN** space creation is presented as a compact titlebar affordance and tab creation is presented in the active-space tab list

#### Scenario: Space slider is compact
- **WHEN** the top Space slider is visible
- **THEN** Space slider titles, short-title controls, and compact indicators use slim borderless styling with selection and hover conveyed without persistent framed cards, section chrome, or a bottom switcher

#### Scenario: Lightweight tab rows
- **WHEN** the active-space tab list contains terminal and alan tabs
- **THEN** each tab appears as a skimmable row with a compact marker, title, secondary context, and low-emphasis status rather than as a card or dashboard tile

#### Scenario: Tab row state hierarchy
- **WHEN** tab rows are displayed in normal, hover, keyboard-focus, and selected states
- **THEN** normal rows sit directly on the sidebar material without a persistent container
- **AND** hover and keyboard-focus rows use only a subtle translucent backing without shadow or scale changes
- **AND** keyboard focus does not introduce the system blue focus ring over the tab row selection surface
- **AND** the selected row uses the strongest rounded selection surface with a light shadow while preserving stable text and accessory alignment
- **AND** selected row surfaces are inset into the sidebar gutter rather than flush to the window edge
- **AND** trailing close affordances appear for selected, hover, or focus states without resizing the row or shifting neighboring rows
- **AND** compact creation rows remain muted by default and gain a subtle backing only on hover or focus

#### Scenario: Space slider scroll boundary
- **WHEN** the active-space tab list is at its resting top position
- **THEN** the top Space slider uses the visual treatment required by the active
  density tier without adding a persistent divider at the slider/list boundary
- **AND** the area between the top Space slider and the first tab row keeps a compact quiet material gap without a persistent divider
- **WHEN** the user scrolls the active-space tab list upward so tab rows move underneath the fixed top Space slider region
- **THEN** alan gradually reveals a subtle divider and downward shadow at the slider/list boundary
- **AND** tab rows clip underneath that boundary instead of drawing over the Space slider

#### Scenario: Visible copy is minimized
- **WHEN** the default sidebar has at least one space and one tab
- **THEN** the sidebar does not rely on persistent explanatory paragraphs, product slogans, keyboard-shortcut labels, redundant `Tabs` and `Spaces` headings, or always-visible creation icons in the Space slider to explain normal operation

#### Scenario: Accessibility remains explicit
- **WHEN** visible explanatory copy is removed from the sidebar
- **THEN** controls, Space slider targets, tab rows, creation buttons, and reduced state cues retain accessibility labels, help text, or menu labels that expose their purpose to assistive technologies

### Requirement: Sidebar actions are progressively disclosed
The default macOS sidebar SHALL keep repeated tab and space rows visually quiet
by showing secondary actions through hover, keyboard focus, context menu, or
compact owner-zone controls rather than always-visible explanatory buttons.

#### Scenario: Tab row default state
- **WHEN** a tab row is visible and not hovered or keyboard focused
- **THEN** the row prioritizes icon, title, compact context, selection, and alan attachment without persistent close/more text buttons or notification dots

#### Scenario: Tab row interaction state
- **WHEN** a tab row is hovered, keyboard focused, or context-clicked
- **THEN** close, more, move, or related secondary actions become available without resizing the row or shifting neighboring content

#### Scenario: Empty sidebar state
- **WHEN** the sidebar has no user-created spaces or no tabs in the active space
- **THEN** the owning zone exposes a compact creation affordance without showing paragraph-style onboarding copy in the default shell

### Requirement: Split tabs expose compact topology
The default macOS sidebar SHALL show a compact split topology indicator on tab
rows whose active tab contains at least one visible terminal pane. The indicator
SHALL communicate pane count, common split topology, and the currently focused
pane when that topology can be mapped to visible pane segments, without
attempting to render exact split ratios or arbitrary tree nesting in the tab row.

#### Scenario: Single-pane tab row
- **WHEN** a tab contains one terminal pane
- **THEN** the tab row shows a compact single-pane topology indicator with stable width

#### Scenario: Two-pane tab row
- **WHEN** a tab contains two visible terminal panes
- **THEN** the tab row shows a compact two-segment indicator that reflects the root split direction and marks the focused pane

#### Scenario: Three-column tab row
- **WHEN** a tab contains three visible terminal panes that normalize to left, middle, and right columns
- **THEN** the tab row shows a compact three-column topology indicator with stable width and a segment-level focused-pane mark when focus is inside one of those panes

#### Scenario: Three-row tab row
- **WHEN** a tab contains three visible terminal panes that normalize to top, middle, and bottom rows
- **THEN** the tab row shows a compact three-row topology indicator with stable width and a segment-level focused-pane mark when focus is inside one of those panes

#### Scenario: Three-pane main stack tab row
- **WHEN** a tab contains three visible terminal panes that normalize to one main pane plus a two-pane stack on the opposite side
- **THEN** the tab row shows a compact main-plus-stack topology indicator that preserves the main pane side or edge and marks the focused pane when focus maps to a displayed segment

#### Scenario: Four-pane recognizable tab row
- **WHEN** a tab contains four visible terminal panes that normalize to a legible four-column, four-row, or 2x2 grid topology
- **THEN** the tab row shows the corresponding compact four-pane topology indicator without widening the tab row, adding text labels, or rendering proportional split ratios

#### Scenario: Complex split tab row
- **WHEN** a tab contains a visible split topology that is not one of the recognized compact topologies or exceeds the legible indicator pane count
- **THEN** the tab row shows a single-pane-shaped topology base with the pane count overlaid on that shape
- **AND** the pane count is not rendered as adjacent text, a separate trailing badge, a notification dot, or a separate sidebar metadata block

#### Scenario: Split tab avoids notification dots
- **WHEN** a non-focused pane inside a split tab needs attention
- **THEN** the split indicator and tab row do not add notification dots, expose raw pane IDs, or add a separate sidebar attention block

#### Scenario: Split topology remains accessible
- **WHEN** assistive technology reads a tab row with a split topology indicator
- **THEN** the accessibility label or help text communicates the pane count and recognized topology in user-facing terms without exposing raw pane IDs or implementation names

### Requirement: Command input opens as a Liquid Glass input
The macOS shell SHALL present `Command-P` as a single floating Liquid
Glass-style input layer that captures text entry without rendering default
candidate sections below the input.

#### Scenario: Command input opens
- **WHEN** the user presses `Command-P` or activates the sidebar command entry
- **THEN** alan opens a floating material-backed input field, focuses the text field, and does not show action, routing, attention, or best-match lists below it

#### Scenario: Command input toggles from shortcut
- **WHEN** the command input is already open and the user presses `Command-P`
- **THEN** alan dismisses the input and returns keyboard focus to the previously focused terminal pane when available

#### Scenario: Command input is visually restrained
- **WHEN** the command input is visible
- **THEN** the surface uses a restrained native material treatment, stable geometry, and compact controls rather than a large card, dashboard panel, or multi-section palette
- **AND** it appears and disappears with an opacity-only fade instead of moving down from the top edge

#### Scenario: Command input dismisses
- **WHEN** the user presses Escape, clicks outside the input, activates a close affordance, or successfully submits a resolved command
- **THEN** alan dismisses the input and returns keyboard focus to the previously focused terminal pane when available

#### Scenario: No default voice affordance
- **WHEN** the command input is visible
- **THEN** the input does not show a microphone or voice-listening affordance unless a future voice-specific requirement explicitly adds one

#### Scenario: Unresolved command stays input-only
- **WHEN** the user submits text that cannot be resolved to a supported command or destination
- **THEN** alan keeps the command surface input-only and communicates the unresolved state without opening candidate rows below the field

### Requirement: Material hierarchy separates navigation from content
The default macOS shell SHALL use material roles that distinguish the functional
navigation/control layer from the content layer. Liquid Glass-style treatment
SHALL be reserved for navigation, command entry, compact controls, and transient
interactive affordances, while workspace and terminal content surfaces SHALL use
standard materials, tonal surfaces, or stable opaque fills that preserve
readability.

#### Scenario: Sidebar uses functional material
- **WHEN** the default shell renders the top Space slider, active-space tab list, and compact sidebar controls
- **THEN** those navigation surfaces use a consistent functional material treatment with legible foreground content and restrained selection states

#### Scenario: Terminal content avoids decorative glass
- **WHEN** the active terminal pane or terminal surround is visible
- **THEN** alan does not apply Liquid Glass-style decorative transparency to the terminal content layer and keeps terminal text contrast stable

#### Scenario: Workspace backdrop is semantic
- **WHEN** the shell renders the main workspace background outside terminal panes
- **THEN** the background uses a semantic material or tonal role chosen for hierarchy rather than hard-coded theme color dominance

### Requirement: Active shell controls use semantic material roles
Buttons, key hints, close controls, hover affordances, and command-entry controls SHALL use
shared semantic material/control roles in the active macOS shell and MUST avoid one-off white,
opaque, or ad hoc translucent fills in default shell chrome.

#### Scenario: Compact icon button
- **WHEN** a compact icon button appears in the sidebar, title bar, terminal chrome, or command entry
- **THEN** its background, hover, pressed, disabled, and selected appearances come from shared shell control roles and keep stable dimensions

#### Scenario: Foreground on material
- **WHEN** text or symbols render on top of a material-backed shell control
- **THEN** alan uses system-vibrant foreground styles or approved shell tokens that remain legible across light appearance, reduced transparency, and increased contrast

#### Scenario: AppKit bridge remains isolated
- **WHEN** a SwiftUI shell view needs an AppKit-backed visual effect material
- **THEN** the view uses a reusable support-layer wrapper rather than creating `NSVisualEffectView` bridge details inline

### Requirement: Active shell surfaces use semantic elevation
The active macOS shell SHALL pair its material roles with a small semantic
radius and shadow scale. Surface elevation MUST communicate hierarchy and
interaction state rather than decorate every translucent control.

#### Scenario: Primary terminal surface anchors elevation
- **WHEN** the active terminal surface is visible
- **THEN** it uses the primary content-surface treatment with continuous 12pt corners, a focused adaptive contact shadow, and restrained rim/highlight treatment

#### Scenario: Static controls stay quiet
- **WHEN** sidebar command launchers, titlebar ghost buttons, or compact static controls are idle
- **THEN** they avoid default shadows and use material tint, stroke, hover, or highlight to show affordance

#### Scenario: Selected navigation uses light elevation
- **WHEN** a sidebar row or Space slider target is selected or previewed
- **THEN** it may use a very light adaptive contact shadow that is smaller than floating overlay shadows and does not produce dirty dark halos in light mode

#### Scenario: Floating surfaces carry stronger elevation
- **WHEN** the command input, pane Find bar, or collapsed sidebar panel floats above the shell
- **THEN** it uses semantic floating-surface shadows that are visible, focused, and adaptive while keeping the terminal content visually dominant

#### Scenario: Radius scale remains role-based
- **WHEN** active shell visual chrome is updated
- **THEN** micro indicators, compact controls, rows, floating inputs, primary surfaces, collapsed panels, and semantic pill inputs use the shared shell radius roles instead of local one-off values

### Requirement: Visible macOS app copy follows product brand identity
The default macOS app UI SHALL render the public product brand as `Alan` and
SHALL use `Alan for macOS` only where platform distinction is useful.

#### Scenario: App chrome is visible
- **WHEN** the Dock name, app menu, window title, toolbar labels, command
  palette labels, sidebar buttons, help text, or accessibility labels name the
  product
- **THEN** they use `Alan`
- **AND** they do not use lowercase `alan`, `AlanNative`, `alanterm`, or
  `Alan Shell` as visible product names

#### Scenario: Terminal app category is visible
- **WHEN** the UI or docs explain the native app's category
- **THEN** they call it a terminal emulator or terminal workspace
- **AND** they do not call the product a shell

### Requirement: Pinned sidebar motion is continuous and coordinated
Pinned sidebar collapse and expansion SHALL be represented as a coordinated
motion of the sidebar surface, workspace inset, lightweight sidebar titlebar
controls, and standard macOS traffic-light controls rather than as independent
insertions, removals, or frame jumps.

The shell SHALL derive pinned, collapsed, floating, and floating-to-pinned
sidebar presentation from one presentation model so the visible sidebar surface
and window chrome share one transition state.

#### Scenario: Sidebar collapses
- **WHEN** the user hides the pinned sidebar and reduced motion is disabled
- **THEN** the sidebar surface moves or narrows out with a short, crisp animation
- **AND** the terminal workspace adjusts its leading inset continuously with the sidebar motion
- **AND** lightweight sidebar titlebar controls and standard macOS traffic-light controls move with the same visual timing instead of jumping to their final positions

#### Scenario: Sidebar expands
- **WHEN** the user pins or expands the sidebar and reduced motion is disabled
- **THEN** the sidebar surface, terminal workspace inset, lightweight sidebar titlebar controls, and standard macOS traffic-light controls move together with a short, non-dragging animation
- **AND** the expanded state settles without delayed toolbar drift or terminal content relayout after the visual motion has completed

#### Scenario: Revealed floating sidebar pins without hiding first
- **WHEN** the sidebar is collapsed, the floating sidebar panel is revealed, and the user chooses Pin Sidebar from that visible panel
- **THEN** alan morphs the visible floating surface into the pinned sidebar position instead of first hiding the floating panel and then expanding a separate pinned surface
- **AND** no rendered frame shows the sidebar absent, offscreen, or duplicated between the floating panel and pinned surface
- **AND** the terminal workspace inset opens continuously during the morph rather than jumping after the panel disappears

#### Scenario: Unified presentation owns chrome during pin morph
- **WHEN** a revealed floating sidebar is pinning into the pinned layout
- **THEN** the lightweight titlebar controls and standard macOS traffic-light controls follow the same interpolated sidebar surface origin
- **AND** traffic lights remain native AppKit controls rather than SwiftUI replicas
- **AND** the final pinned state clears transient floating reveal state only after the visible morph has settled

#### Scenario: Reduced motion collapse
- **WHEN** reduced motion is enabled and the pinned sidebar is hidden or shown
- **THEN** alan avoids springy movement while still applying one coherent final layout for sidebar surface, workspace inset, titlebar controls, and traffic-light controls

#### Scenario: Native traffic-light behavior preserved
- **WHEN** sidebar or titlebar chrome moves during pinned or floating sidebar transitions
- **THEN** alan continues using the standard macOS traffic-light controls for close, minimize, and zoom behavior rather than drawing custom replacements

### Requirement: Activity UI Is Compact And Terminal First
Terminal activity UI SHALL use compact pane title-bar accessories, sidebar tab
metadata, and accessibility values instead of dashboard panels, persistent
bottom status strips, or debug labels in the default shell.

#### Scenario: Pane reports progress
- **WHEN** a pane reports determinate, indeterminate, paused, or failed
  progress
- **THEN** Alan presents that state as a lightweight pane title-bar accessory or
  sidebar activity rail that does not resize the terminal canvas, toolbar
  content, or split dividers

#### Scenario: Pane reports agent status
- **WHEN** a pane running a supported CLI coding agent reports running, needs
  input, complete, or error state
- **THEN** Alan presents a compact user-facing status in the pane title-bar and
  sidebar row without exposing raw event names, hook payloads, session IDs, or
  debug implementation details

#### Scenario: No actionable activity exists
- **WHEN** a tab has no active, notable, or user-actionable terminal activity
- **THEN** the sidebar tab row shows worktree and branch context as its
  secondary line instead of reserving empty activity chrome or showing idle or
  success states

### Requirement: Sidebar Tab Rows Are Attention-Oriented Work Rows
Sidebar tab rows SHALL use a richer but restrained layout that helps users
identify a tab and decide whether it needs attention.

#### Scenario: Tab row default layout
- **WHEN** Alan renders a sidebar tab row
- **THEN** the row contains a leading topology or kind slot, a title line, one
  secondary line that shows activity or worktree/branch context, and an
  optional progress rail only when the displayed activity owns progress

#### Scenario: Close affordance appears
- **WHEN** a sidebar tab row is hovered, keyboard-focused, or otherwise
  interaction-active
- **THEN** the close affordance appears as a trailing overlay without reserving
  a permanent layout slot in the row content

#### Scenario: Close affordance is hidden
- **WHEN** a sidebar tab row is not interaction-active
- **THEN** title, secondary text, activity, and progress content may occupy the
  full row width without leaving a fixed empty close-button column

#### Scenario: Split tab leading slot
- **WHEN** a tab contains multiple visible panes
- **THEN** the leading slot shows split topology instead of the generic terminal
  icon, and activating that topology cycles focus through panes in a stable
  order

#### Scenario: Single-pane leading slot
- **WHEN** a tab contains one pane
- **THEN** the leading slot shows the tab kind or supported agent icon rather
  than a split topology indicator

#### Scenario: Activity takes precedence over context
- **WHEN** a tab has sidebar-worthy activity
- **THEN** the secondary line shows the source-first activity copy instead of
  worktree or branch context

#### Scenario: No activity fallback
- **WHEN** a tab has no sidebar-worthy activity
- **THEN** the secondary line uses worktree or repository leaf plus branch when
  available, with cwd leaf only as a fallback

### Requirement: Pane Title Bars Own Pane Detail
Pane title bars SHALL keep terminal title as the primary label and expose
pane-local context through accessories.

#### Scenario: Pane title bar with activity
- **WHEN** a pane has pane-local activity, cwd or worktree context, branch,
  process, or supported agent state
- **THEN** the pane title bar presents that detail as compact accessories while
  keeping the terminal title as the primary text

#### Scenario: Sidebar and pane title differ
- **WHEN** the sidebar tab row shows a tab-level activity from another pane
- **THEN** the focused pane title bar still shows only the focused pane's own
  local detail and does not mirror unrelated tab-level activity

### Requirement: Activity Notifications Are Low Noise
System and in-app notifications for terminal activity SHALL be reserved for
actionable, out-of-view, or user-configured events.

#### Scenario: Background agent needs input
- **WHEN** a background or unfocused pane's supported coding agent needs user
  input
- **THEN** Alan may notify the user and mark the owning tab without stealing
  focus or opening a new panel

#### Scenario: Foreground progress updates
- **WHEN** the focused visible pane emits progress updates
- **THEN** Alan updates visible activity UI without sending system
  notifications by default

#### Scenario: Long command completes while unfocused
- **WHEN** a long-running command completes in an unfocused pane and the event
  meets the notification policy
- **THEN** Alan may send a concise command-completion notification with success
  or failure state

#### Scenario: Foreground command succeeds
- **WHEN** a command succeeds in the focused visible pane
- **THEN** Alan does not send a system notification by default

#### Scenario: Agent fails in background
- **WHEN** a supported coding agent reports failure or error in a background or
  unfocused pane
- **THEN** Alan may send a concise notification and mark the owning tab

### Requirement: Quick Terminal Presentation Is Native And Lightweight
The quick terminal SHALL present as a detached, lightweight native macOS Peak
window that preserves Alan's terminal-first shell design and avoids dashboard or
floating-card composition.

#### Scenario: Quick terminal appears
- **WHEN** the quick terminal is summoned
- **THEN** Alan presents a focused terminal surface with restrained native
  material chrome, no inspector, no marketing-style header, and no duplicate
  sidebar

#### Scenario: Quick terminal appears outside the main window
- **WHEN** the user summons the quick terminal from any macOS Space
- **THEN** Alan presents the Peak on the current active display and Space
  without requiring, attaching to, or raising Alan's main window

#### Scenario: Quick terminal hides
- **WHEN** the quick terminal is dismissed without closing the session
- **THEN** Alan hides the presentation without changing regular shell sidebar,
  tab, split, or terminal geometry

#### Scenario: Terminal keys remain terminal keys
- **WHEN** the quick terminal owns focus and the user presses `Esc`
- **THEN** Alan sends the key to the terminal surface instead of hiding the Peak
  by default

#### Scenario: Focus changes outside the Peak
- **WHEN** the quick terminal loses focus because the user clicks another app or
  window
- **THEN** Alan keeps the Peak visible until the user invokes the quick-terminal
  toggle, hide command, close command, or promotion action

#### Scenario: Quick terminal can become a normal tab
- **WHEN** the user opens the Peak's `Open in Space` affordance
- **THEN** Alan offers Alan space destinations without duplicating sidebar
  chrome inside the Peak

#### Scenario: Quick terminal activity exists
- **WHEN** the hidden quick terminal has user-actionable activity
- **THEN** Alan surfaces that activity through the same compact activity and
  notification policy used for regular terminal panes

### Requirement: Tab Organization Follows Lightweight Arc-Like Sections
The macOS sidebar SHALL present per-Space Pinned and Unpinned Tab sections with
a restrained Arc-like visual treatment that preserves scan speed and avoids
heavy group chrome.

#### Scenario: Pinned and Unpinned sections render
- **WHEN** a Space contains Pinned and Unpinned Tabs
- **THEN** alan separates the sections with subtle spacing or a divider rather
  than large boxed panels, cards, or heavy section headers

#### Scenario: Tab rows remain stable
- **WHEN** the user hovers, selects, drags, pins, unpins, or reorders Tabs
- **THEN** Tab rows keep stable height and sidebar geometry without resizing
  terminal content

#### Scenario: New Tab remains lightweight
- **WHEN** the sidebar shows the New Tab affordance
- **THEN** it appears as a lightweight list action rather than a large toolbar
  or dashboard-style primary button

#### Scenario: No folder scope
- **WHEN** the first Tab organization pass ships
- **THEN** alan does not introduce tab folders, nested tab groups, or a global
  pinned shelf

### Requirement: Drag Feedback Is Direct And Minimal
Tab drag feedback SHALL communicate target section and insertion position
without explanatory copy or persistent drag chrome.

#### Scenario: Drag insertion preview
- **WHEN** the user drags a Tab row over a valid insertion point
- **THEN** alan shows a direct insertion preview in the target section

#### Scenario: Drag crosses section boundary
- **WHEN** the user drags a Tab row from Pinned to Unpinned or Unpinned to
  Pinned
- **THEN** the target section and insertion position are visually clear before
  drop

#### Scenario: Invalid drag target
- **WHEN** the user drags over a target that cannot accept the Tab
- **THEN** alan avoids committing the mutation and preserves current Tab order
  without showing raw debug identifiers

### Requirement: Non-terminal content stays inside shell workspace chrome
Markdown、settings 和未来 content surface SHALL 继承 alan macOS shell 的 sidebar、
toolbar、tab selection、split layout 和 restrained material 视觉系统，而不是引入第二套 page
chrome、dashboard 布局或营销式页面结构。

#### Scenario: Markdown tab is active
- **WHEN** 用户选择 markdown content tab
- **THEN** 主区域显示 markdown viewer
- **AND** sidebar、toolbar 和 tab row 仍保持默认 shell chrome
- **AND** UI 不显示 terminal-specific debug labels 或 raw content IDs

#### Scenario: Settings tab is active
- **WHEN** 用户选择 alan settings content tab
- **THEN** 设置内容呈现在 shell content area 中
- **AND** 默认 UI 不增加 page-like hero、card-heavy dashboard 或独立 settings navigation shell

### Requirement: Content labels are user-facing
默认 UI SHALL 使用 content title、file name、settings section 或未来 content title 等用户可见信息
展示 tab/pane，不得把 implementation IDs 作为主要标签。

#### Scenario: Mixed split pane title bars render
- **WHEN** 一个 split tab 同时显示 terminal、markdown 和 settings panes
- **THEN** 每个 pane title bar 显示对应用户可见标题和必要的 compact status
- **AND** terminal-only 状态只出现在 terminal pane 上

#### Scenario: Command results include non-terminal content
- **WHEN** command input 列出 markdown 或 settings target
- **THEN** 结果使用用户可见 content title 和 type hint
- **AND** 不以 raw pane ID、content ID 或 renderer class name 作为 primary label

### Requirement: Sidebar interactions do not drag the window
The macOS shell SHALL limit primary-window drag movement to explicit blank
titlebar/chrome areas. Sidebar workspace controls SHALL own their own
interactions and MUST NOT move the window when the user drags them.

#### Scenario: Tab row drag reorders without moving window
- **WHEN** the user drags a tab row in the sidebar tab list
- **THEN** alan treats the gesture as a tab selection, reorder, or drop-target
  interaction according to the tab-list contract
- **AND** the primary shell window does not move as part of that drag

#### Scenario: Space controls are interaction surfaces
- **WHEN** the user drags or clicks within the top Space slider, command
  launcher, or sidebar titlebar controls
- **THEN** alan routes the event to the relevant sidebar control
- **AND** the primary shell window does not move because of window-background
  dragging

#### Scenario: Blank chrome still moves the window
- **WHEN** the user drags an empty, non-control area of the top titlebar/chrome
  region
- **THEN** alan moves the primary shell window using the native macOS window
  drag behavior
- **AND** double-clicking the same empty chrome region continues to toggle the
  visible-frame zoom behavior

### Requirement: UI conformance has repeatable smoke evidence
Mac shell UI conformance work SHALL include repeatable smoke or screenshot
evidence for launch, space/tab switching, split creation, command UI, and
pane-scoped Find behavior.

#### Scenario: Default launch evidence
- **WHEN** a UI conformance implementation is ready
- **THEN** maintainers can run or inspect a smoke artifact showing the light-mode default window with material sidebar and terminal-first content

#### Scenario: Command UI evidence
- **WHEN** command UI behavior changes
- **THEN** maintainers can run or inspect evidence showing `Go to or Command...` results with user-facing labels

#### Scenario: Find evidence
- **WHEN** pane-scoped Find behavior changes
- **THEN** maintainers can run or inspect evidence confirming the active pane shows a native-feeling Find surface without restoring inspector chrome or debug-first panels

### Requirement: Automation surfaces do not add default chrome
Adding App Intents and automation support SHALL not add visible default UI chrome
or explanatory panels to the terminal workflow.

#### Scenario: App Intents installed
- **WHEN** automation support is present in the app
- **THEN** the default shell window remains terminal-first and does not show automation setup cards, implementation jargon, or dashboard sections

#### Scenario: Intent result activates app
- **WHEN** an App Intent activates a shell target
- **THEN** the window opens to the relevant space, tab, or pane using normal shell UI rather than a special automation debug surface

### Requirement: Settings Manages Terminal Profiles Locally
Alan macOS Settings SHALL provide a local Terminal Profiles surface for
creating, editing, and choosing the default Terminal Profile without presenting
Terminal Profiles as provider accounts.

#### Scenario: Terminal Profiles appear in Settings
- **WHEN** the user opens Settings
- **THEN** alan shows Terminal Profiles as local terminal startup configuration
- **AND** alan does not place Terminal Profiles under provider Accounts or label
  them as connection profiles

#### Scenario: Structured profile editing
- **WHEN** the user edits a Terminal Profile
- **THEN** alan provides structured controls for login shell, sudo Unix user,
  sudo root, and custom command launch modes
- **AND** required fields are validated before the profile is saved

#### Scenario: Sudo behavior is explained without raw sudoers editing
- **WHEN** the user configures a sudo Unix user or sudo root Terminal Profile
- **THEN** alan explains that sudo prompts and passwordless sudo behavior are
  controlled by the operating system
- **AND** the Terminal Profile editor does not offer raw sudoers-file editing

### Requirement: Spaces Expose Terminal Profile Binding
The macOS shell UI SHALL let users view and change a Space's default Terminal
Profile through compact shell-native affordances.

#### Scenario: Space profile can be selected
- **WHEN** the user opens a Space action menu or profile selector
- **THEN** alan lists local Terminal Profiles by title and launch kind
- **AND** selecting one updates the Space's default `terminal_profile_id`

#### Scenario: Space profile hint stays quiet
- **WHEN** a Space has a Terminal Profile binding
- **THEN** alan may show a compact icon, color, or label hint in the sidebar
- **AND** alan keeps the sidebar scannable and avoids turning Space rows into
  dense configuration panels

#### Scenario: Missing profile is visible
- **WHEN** a Space or terminal content references a missing Terminal Profile
- **THEN** alan shows a missing-profile state with the missing id
- **AND** alan keeps terminal creation available through login-shell fallback

### Requirement: Terminal Profile Details Stay Appropriately Redacted
Alan macOS shell UI SHALL expose Terminal Profile identity in normal shell
surfaces without leaking unnecessary command details.

#### Scenario: Custom command is not shown in normal sidebar rows
- **WHEN** a terminal content uses a `custom_command` Terminal Profile
- **THEN** normal sidebar and pane chrome use the profile title or kind
- **AND** the full custom command is shown only in Settings or explicit
  diagnostics

#### Scenario: Root profile is visibly distinct
- **WHEN** terminal content uses a sudo root Terminal Profile
- **THEN** alan presents a restrained but clear root identity indicator in
  terminal chrome or status surfaces

### Requirement: Settings Presents Managed Terminal Account Provisioning
Alan macOS Settings SHALL provide a Managed Terminal Account flow for creating
terminal-only local users and SHALL distinguish it from macOS GUI automatic
login.

#### Scenario: Provision action is local and explicit
- **WHEN** the user opens Terminal Profile or local terminal identity settings
- **THEN** alan offers an explicit action to create or repair a Managed Terminal
  Account
- **AND** alan labels the flow as terminal account provisioning, not autologin

#### Scenario: GUI automatic login is not implied
- **WHEN** the provisioning flow describes the result
- **THEN** alan states that it does not enable macOS GUI automatic login
- **AND** alan describes the result as passwordless terminal entry from the
  current GUI user to the target Unix user

#### Scenario: Privileged plan is reviewed before apply
- **WHEN** the user reaches the apply step
- **THEN** alan shows the planned privileged changes in compact user-facing
  language
- **AND** the user must confirm before alan applies account, sudoers, or Terminal
  Profile changes

### Requirement: Provisioning UI Surfaces Safety State
Alan macOS Settings SHALL surface readiness, repair, and rollback state for
Managed Terminal Accounts without exposing passwords or raw privileged command
payloads in normal UI.

#### Scenario: Ready account is shown
- **WHEN** a Managed Terminal Account is verified ready
- **THEN** alan shows the account as ready for terminal entry
- **AND** alan links it to the matching Terminal Profile when one exists

#### Scenario: Repairable account is shown
- **WHEN** a Managed Terminal Account is partially provisioned or fails
  verification
- **THEN** alan shows a repairable state with the failed step
- **AND** alan offers to preview a repair plan

#### Scenario: Passwords are not displayed
- **WHEN** provisioning uses generated or administrator-entered passwords
- **THEN** alan does not show those passwords in Settings after the operation
- **AND** alan does not write them into normal shell state, workspace manifests,
  or Terminal Profile definitions
