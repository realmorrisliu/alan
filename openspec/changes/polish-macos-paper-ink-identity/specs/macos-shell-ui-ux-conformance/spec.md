## MODIFIED Requirements

### Requirement: Default UI hides implementation jargon
The default macOS UI SHALL avoid exposing raw pane IDs, `tab_id`, binding,
runtime phases, `window attached`, `title updated`, and other implementation
terms outside explicit debug surfaces. It SHALL also avoid obsolete product
labels from legacy native app builds in visible app chrome. Visible machine
facts (filesystem paths, git branches, process names, counts, key hints) SHALL
render in the mono type track and human-facing copy in the proportional track.

#### Scenario: Normal terminal workflow
- **WHEN** a user creates, selects, splits, or closes tabs and panes
- **THEN** visible copy uses product terms such as Space, Tab, Split, Find, and
  Open in alan where applicable
- **AND** visible copy does not expose Ask alan or New alan Tab as default
  shell product actions

#### Scenario: Machine facts use the mono accent track
- **WHEN** the sidebar tab-row secondary line or a pane title-bar accessory
  shows a filesystem path, working-directory leaf, git branch, or process name
- **THEN** that text renders in the mono type track (`ShellType.mono`)
- **AND** human-language status phrases (for example renderer failure,
  read-only, needs attention, content-type hints) and activity copy render in
  the proportional track (`ShellType.pro`)

#### Scenario: Debug surfaces
- **WHEN** implementation details are needed
- **THEN** they remain in explicit debug-only surfaces, logs, scripts, or
  snapshots rather than default shell chrome

### Requirement: Space slider supports adaptive density and scrub navigation
The default macOS shell Space slider SHALL use a continuous rounded track that
adapts Space target widths to available sidebar space, supports every Space
without an arbitrary count cap, and preserves preview-first scrub navigation
without hover-driven geometry changes or cover-flow motion.

### Requirement: Empty Spaces render as workspace placeholders
The default macOS shell SHALL render a selected Space with no mounted content
as a centered workspace placeholder on the raised paper panel rather than as an
empty terminal surface or a left-hugging fragment.

#### Scenario: Empty Space placeholder composition
- **WHEN** the selected Space has no selected tab, pane tree, or mounted
  content
- **THEN** alan shows a centered placeholder whose heading is the Space title
  (falling back to a generic empty label), a single quiet secondary line, and a
  bordered New Tab control using shared control material
- **AND** a key-hint line may show the new-tab shortcut with the chord in the
  mono track and its description in the proportional track
- **AND** the placeholder is not painted with the terminal dark canvas,
  terminal rim, or terminal surface shadow

#### Scenario: Empty Space primary action remains terminal-first
- **WHEN** the user activates the empty Space New Tab action
- **THEN** alan creates a normal terminal tab in the current Space through the
  existing tab creation path

### Requirement: Sidebar Tab Rows Are Attention-Oriented Work Rows
Sidebar tab rows SHALL use a richer but restrained layout that helps users
identify a tab and decide whether it needs attention. The single-pane leading
slot SHALL show the tab kind without using the focus accent as a selection
marker, while the split-pane leading slot keeps the interactive topology
indicator.

#### Scenario: Single-pane leading slot does not borrow the focus accent
- **WHEN** a tab contains one pane and its row is selected
- **THEN** the single-pane indicator uses a neutral ink fill rather than the
  focus accent color
- **AND** selection is conveyed by the row surface treatment, not by an
  indigo-filled indicator

#### Scenario: Split-pane focus marker is preserved
- **WHEN** a tab contains multiple visible panes
- **THEN** the focused pane within the split topology indicator may still use
  the focus accent to mark focus, which is a focus signal rather than a
  selection marker

### Requirement: Activity UI Is Compact And Terminal First
Terminal activity UI SHALL use compact pane title-bar accessories, sidebar tab
metadata, and accessibility values instead of dashboard panels, persistent
bottom status strips, or debug labels in the default shell. The action color
SHALL be reserved for states that require the user to act and SHALL NOT mark
quiet, idle, or merely-active panes and Spaces.

#### Scenario: Quiet panes and Spaces stay silent
- **WHEN** a Space or tab has panes that are running or idle but have no
  state requiring user action
- **THEN** its sidebar icon and row do not render the action color
- **AND** the action color appears only when an agent or command is blocked on
  the user, or a failure requires user intervention
