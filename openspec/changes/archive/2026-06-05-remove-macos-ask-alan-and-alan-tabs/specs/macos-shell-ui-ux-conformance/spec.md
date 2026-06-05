## ADDED Requirements

### Requirement: Ask alan and alan tab surfaces are absent
The default macOS shell UI SHALL NOT expose Ask alan, floating command input, or
first-party alan tab creation surfaces.

#### Scenario: Default shell opens
- **WHEN** the macOS shell window is visible
- **THEN** the sidebar, titlebar, toolbar, menus, context menus, and default
  workspace chrome do not show `Ask alan...`, `Go to or Command...`, `New alan
  tab`, or another first-party alan tab creation entry point

#### Scenario: Command-P is pressed
- **WHEN** the user presses `Command-P`
- **THEN** the macOS shell does not open an Alan-owned floating command input,
  command palette, Ask alan surface, or replacement launcher

#### Scenario: Agent is needed
- **WHEN** the user wants to run Alan or another coding agent from the app
- **THEN** the supported path is to use a normal terminal tab and run the
  desired CLI command inside that terminal

## MODIFIED Requirements

### Requirement: Default UI hides implementation jargon
The default macOS UI SHALL avoid exposing raw pane IDs, `tab_id`, binding,
runtime phases, `window attached`, `title updated`, and other implementation
terms outside explicit debug surfaces. It SHALL also avoid obsolete product
labels from legacy native app builds in visible app chrome.

#### Scenario: Normal terminal workflow
- **WHEN** a user creates, selects, splits, or closes tabs and panes
- **THEN** visible copy uses product terms such as Space, Tab, Split, Find, and
  Open in alan where applicable
- **AND** visible copy does not expose Ask alan or New alan Tab as default
  shell product actions

#### Scenario: Removed command input routing states
- **WHEN** default shell UI is visible
- **THEN** alan does not show command-input routing states, typed command
  submissions, unresolved command status, routing-candidate rows, attention
  candidate rows, best-match rows, or command-row sections below a field

#### Scenario: Debug surfaces
- **WHEN** implementation details are needed
- **THEN** they remain in explicit debug-only surfaces, logs, scripts, or
  snapshots rather than default shell chrome

### Requirement: Toolbar is native and restrained
The macOS toolbar/titlebar SHALL feel like native window chrome and contain only
the current tab title/context and a small number of frequent terminal workspace
actions.

#### Scenario: Toolbar default state
- **WHEN** no urgent attention item exists
- **THEN** the toolbar does not show attention as a large standalone primary control

#### Scenario: Command entry removed
- **WHEN** the default shell toolbar, titlebar, sidebar, or menu chrome is visible
- **THEN** alan does not show a persistent command entry, Ask alan button,
  floating command input launcher, or Command-P hint

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

### Requirement: Radius normalization preserves shell hierarchy
Radius normalization SHALL make alan feel calmer and more precise without
turning the UI into a flat grid or weakening control affordances.

#### Scenario: Sidebar remains skimmable
- **WHEN** sidebar spaces, tabs, and creation controls are visible
- **THEN** smaller radii preserve row scanning, hover states, selected states, and stable dimensions

#### Scenario: Removed command input has no radius contract
- **WHEN** default shell UI radius conformance is reviewed
- **THEN** the review does not preserve or reintroduce floating Ask alan command
  input surfaces, text fields, close controls, or inline unresolved states

#### Scenario: Overlays remain secondary
- **WHEN** the Find bar or another remaining default-shell overlay is visible
- **THEN** that surface uses restrained radii and does not read as a large decorative card competing with the terminal

### Requirement: Sidebar matches single-column space/tab navigation
The default macOS sidebar SHALL remain a single vertical navigation column that
aligns cleanly around the macOS traffic-light area, with a restrained initial
width around 264 pt. Spaces SHALL be switched through a compact bottom
borderless icon switcher and horizontal sidebar swipe gestures, while tabs for
the active space remain the primary sidebar list.
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
- **THEN** the sidebar reads as an active-space tab list and bottom space
  switcher in one vertical column rather than as unrelated dashboard sections,
  a two-column sidebar, or an Ask alan launcher above the tab list
- **AND** the sidebar surface has a cool material tint that remains coherent across empty space, controls, rows, and the bottom switcher

#### Scenario: Space selection
- **WHEN** a user selects a space in the bottom switcher
- **THEN** the tab list updates to show only tabs belonging to that active space

#### Scenario: Sidebar swipe switches spaces
- **WHEN** a user performs a clear horizontal swipe gesture inside the sidebar
- **THEN** alan previews the previous or next space with gesture-tracked motion across the sidebar header and tab list
- **AND** the preview is rendered from horizontal finger translation across the full sidebar page width rather than from threshold-derived progress
- **AND** the active-space title pager uses the same full-width movement as the tab list rather than a narrowed header row
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
- **THEN** space creation is presented as a compact bottom-switcher affordance and
  terminal tab creation is presented in the active-space tab list or toolbar context
- **AND** alan tab creation is not presented as a sidebar action

#### Scenario: Space switcher is borderless
- **WHEN** the bottom space switcher is visible
- **THEN** space buttons use slim borderless icon styling with selection and hover conveyed without persistent framed cards, section chrome, or notification dots

#### Scenario: Lightweight tab rows
- **WHEN** the active-space tab list contains terminal or non-terminal content tabs
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

#### Scenario: Space title scroll boundary
- **WHEN** the active-space tab list is at its resting top position
- **THEN** the active-space title appears as a quiet grayscale label without a persistent pill or control background
- **AND** the area between the space title label and the first tab row keeps a compact quiet material gap without a persistent divider
- **WHEN** the user scrolls the active-space tab list upward so tab rows move underneath the fixed space title region
- **THEN** alan gradually reveals a subtle divider and downward shadow at the title/list boundary
- **AND** tab rows clip underneath that boundary instead of drawing over the space title

#### Scenario: Visible copy is minimized
- **WHEN** the default sidebar has at least one space and one tab
- **THEN** the sidebar does not rely on persistent explanatory paragraphs, product slogans, keyboard-shortcut labels, redundant `Tabs` and `Spaces` headings, or always-visible creation icons in the space-title row to explain normal operation

#### Scenario: Accessibility remains explicit
- **WHEN** visible explanatory copy is removed from the sidebar
- **THEN** controls, space switcher items, tab rows, creation buttons, and reduced state cues retain accessibility labels, help text, or menu labels that expose their purpose to assistive technologies

### Requirement: Toolbar stays restrained during split interactions
Advanced split, focus, resize, equalize, close, and pane lift affordances SHALL
not turn the toolbar into a dense control strip.

#### Scenario: Multiple panes visible
- **WHEN** a tab contains multiple panes
- **THEN** the default toolbar remains focused on current tab context and
  frequent terminal workspace actions

#### Scenario: Pane lift available
- **WHEN** pane lift is available through an explicit non-terminal affordance
- **THEN** the default toolbar does not add a persistent pane-management strip

### Requirement: UI conformance has repeatable smoke evidence
Mac shell UI conformance work SHALL include repeatable smoke or screenshot
evidence for launch, space/tab switching, split creation, pane-scoped Find
behavior, and the absence of removed Ask alan and alan-tab surfaces.

#### Scenario: Default launch evidence
- **WHEN** a UI conformance implementation is ready
- **THEN** maintainers can run or inspect a smoke artifact showing the light-mode default window with material sidebar and terminal-first content

#### Scenario: Removed Ask alan evidence
- **WHEN** Ask alan or alan-tab removal is marked complete
- **THEN** maintainers can inspect evidence that the default shell has no
  `Ask alan...`, `Go to or Command...`, Command-P command input, or `New alan
  tab` surface

#### Scenario: Find evidence
- **WHEN** pane-scoped Find behavior changes
- **THEN** maintainers can run or inspect evidence confirming the active pane shows a native-feeling Find surface without restoring inspector chrome or debug-first panels

## REMOVED Requirements

### Requirement: Command UI owns navigation and shell actions
**Reason**: The typed command input and Ask alan floating command surface are no
longer part of the macOS shell product.

**Migration**: Use native menu, keyboard, context-menu, sidebar, and terminal
surfaces for supported workspace actions. Users can run Alan from a normal
terminal tab through the CLI.

### Requirement: Command input opens as a Liquid Glass input
**Reason**: The Command-P floating command input is being removed rather than
restyled.

**Migration**: Remove the surface and its Liquid Glass-specific UI contract.
