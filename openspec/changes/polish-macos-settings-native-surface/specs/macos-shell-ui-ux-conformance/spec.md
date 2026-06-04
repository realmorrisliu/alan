## ADDED Requirements

### Requirement: Settings Uses Native Source List And Preference Detail
Alan macOS Settings SHALL present its internal navigation as a compact native
source list and SHALL present the selected group as a compact sectioned
preference list inside the existing shell content area.

#### Scenario: Settings opens with native hierarchy
- **WHEN** the user opens the Settings content tab in light mode
- **THEN** alan shows the existing Settings groups in a compact source-list navigation
- **AND** the Settings pane title, navigation rail, and page backdrop share a shallow native gray plane
- **AND** the selected group renders as direct preference sections rather than a stable white page sheet or web-style card page
- **AND** the Settings surface remains inside the shell content area without creating a separate preferences window or dashboard shell

#### Scenario: Selected group uses direct preference geometry
- **WHEN** the Settings window is wider than the selected group's content
- **THEN** alan keeps the selected group's row content column left anchored with a stable maximum width
- **AND** alan gives the content enough width for developer metadata such as paths, endpoints, and namespaces
- **AND** alan uses section titles and horizontal dividers to create hierarchy instead of container cards or a page sheet
- **AND** alan does not stretch sparse settings rows across the full detail pane
- **AND** alan does not compensate for sparse settings with large blank card surfaces

#### Scenario: Navigation remains subordinate
- **WHEN** Settings renders the internal navigation
- **THEN** the navigation uses source-list row selection, restrained icon and label sizing, and subtle material depth
- **AND** the navigation rail contains only General, Terminal, Agent, and System, without a duplicate internal Settings title
- **AND** the navigation list starts 24pt below the Settings content top with 12pt leading inset and 8pt trailing inset
- **AND** each navigation row uses 30pt row height with approximately 13pt icon and 13pt label sizing
- **AND** the selected source-list row uses a macOS-style capsule fill with active text/icon state and no blue accent bar
- **AND** selected navigation text and icons become primary rather than blue-emphasized
- **AND** alan does not present the internal Settings navigation as a second app-level sidebar, large tab bar, or stack of web buttons

### Requirement: Settings Rows Use Precise Native Form Rhythm
Alan macOS Settings rows SHALL use disciplined app-UI typography, stable columns,
and restrained dividers so settings can be scanned like a developer control
panel rather than a dashboard card.

#### Scenario: Row columns align
- **WHEN** a selected Settings group renders multiple rows
- **THEN** label, description, value, action, toggle, and segmented-control positions align consistently across rows in that group
- **AND** rows use one native setting template: title, optional secondary text, and optional trailing control
- **AND** read-only System metadata values render as secondary text below the label rather than as a far-right table column
- **AND** toggles, segmented controls, and button actions share a bounded trailing control column instead of hugging the full 760pt content edge
- **AND** long metadata values expose the full value through native help or an explicit Copy/Show action

#### Scenario: System rows expose real actions
- **WHEN** the System group presents local endpoint, path, and diagnostics rows
- **THEN** alan exposes compact actions for rows with natural local operations, such as copying the daemon endpoint and opening local folders
- **AND** daemon endpoint uses a native Copy button rather than blue link styling
- **AND** local folder actions use native wording such as Show... rather than web-style external-link arrows
- **AND** diagnostics remains a real toggle plus export action
- **AND** install facts such as Channel and Updates remain honest read-only values rather than disabled or fake edit controls
- **AND** update explanations and implementation details do not appear as always-visible copy when the label/value pair is already clear

#### Scenario: Row descriptions clarify scope
- **WHEN** a row label is ambiguous without context, such as Sidebar or Inactive split dimming
- **THEN** alan provides concise secondary copy that explains the affected surface
- **AND** the secondary copy stays visually subordinate to the row label

#### Scenario: Typography roles remain native and scannable
- **WHEN** Settings renders its source list and selected group's rows
- **THEN** alan uses distinct typography roles for source-list labels, page titles, section labels, row labels, row descriptions, and trailing values
- **AND** row descriptions and trailing values remain visually subordinate to row labels through size, weight, and muted blue-gray ink
- **AND** row labels use restrained native weight rather than reading as page headings
- **AND** the selected source-list item emphasizes the label and active icon without turning every navigation label blue

#### Scenario: Dense row rhythm is preserved
- **WHEN** a selected Settings group contains only a small number of rows
- **THEN** alan keeps row height, section spacing, and typography compact enough that the surface feels intentional instead of empty
- **AND** section spacing stays close to a control-panel rhythm rather than leaving large About-page gaps between sections
- **AND** alan does not compensate for sparse content with oversized headers, hero spacing, or large decorative panels

### Requirement: Settings Surface Depth Avoids Web Dashboard Chrome
Alan macOS Settings SHALL use subtle native surface depth and restrained accent
color. It SHALL avoid visual treatments that make the surface read as a web
admin page.

#### Scenario: Surface layers are distinguishable
- **WHEN** Settings is visible in the default light appearance
- **THEN** the window/titlebar, Settings source list, detail pane, preference rows, and controls are distinguishable through subtle material, tint, separators, and fill differences
- **AND** the surface does not collapse into one flat white or pale-gray plane

#### Scenario: Accent color is controlled
- **WHEN** Settings shows selected navigation, segmented controls, toggles, or actions
- **THEN** accent color is limited to active state and actionable affordances
- **AND** alan avoids letting multiple bright controls become the dominant visual hierarchy of the page

#### Scenario: Dashboard patterns are absent
- **WHEN** Settings is active
- **THEN** alan does not show card-heavy dashboard composition, nested cards, decorative gradients, drop-shadow panels, marketing copy, or large icon-heading-text blocks

### Requirement: Settings Native Polish Is Visually Verified
MacOS Settings visual polish SHALL be verified with a fresh Alan Dev run before
the implementation tasks are marked complete.

#### Scenario: Fresh Alan Dev screenshot review
- **WHEN** Settings native-polish implementation is ready for review
- **THEN** maintainers can inspect a fresh Alan Dev light-mode screenshot showing capsule source-list navigation, section dividers, unified title/detail/control rows, aligned trailing controls, real System actions, and restrained accent color
- **AND** the screenshot is captured after relaunching Alan Dev rather than reusing a stale running window

#### Scenario: Visual review checks native criteria
- **WHEN** the screenshot is reviewed
- **THEN** maintainers compare it against this change's native-surface criteria: compact capsule source-list selection, direct sectioned preference layout, left-anchored 760pt maximum-width content, unified setting rows, subordinate read-only metadata values, bounded trailing controls, subtle surface depth, no card-heavy dashboard chrome, and no oversized web-page spacing
