## ADDED Requirements

### Requirement: Settings Uses Native Source List And Grouped Detail
Alan macOS Settings SHALL present its internal navigation as a compact native
source list and SHALL present the selected group as a compact grouped settings
form inside the existing shell content area.

#### Scenario: Settings opens with native hierarchy
- **WHEN** the user opens the Settings content tab in light mode
- **THEN** alan shows the existing Settings groups in a compact source-list navigation
- **AND** the Settings pane title, navigation rail, and page backdrop share a shallow native gray plane
- **AND** the selected group renders inside a stable white page sheet rather than a web-style card page
- **AND** the Settings surface remains inside the shell content area without creating a separate preferences window or dashboard shell

#### Scenario: Selected group uses stable sheet geometry
- **WHEN** the Settings window is wider than the selected group's content
- **THEN** alan aligns the white Settings page sheet's top border with the bottom edge of the pane title bar
- **AND** alan keeps the white Settings page sheet inset from the detail pane's right edge by 8px
- **AND** alan keeps the white Settings page sheet inset from the detail pane's bottom edge by 8px
- **AND** alan uses no outer drop shadow on the white Settings page sheet
- **AND** alan may use only a thin border and focused inner edge treatment to focus the sheet
- **AND** alan keeps the selected group's row content column left anchored with a stable maximum width inside the sheet
- **AND** alan does not stretch sparse settings rows across the full sheet

#### Scenario: Navigation remains subordinate
- **WHEN** Settings renders the internal navigation
- **THEN** the navigation uses source-list row selection, restrained icon and label sizing, and subtle material depth
- **AND** the navigation keeps compact leading, trailing, and top insets relative to the Settings pane
- **AND** the first source-list row's visible top edge optically aligns with the white Settings page sheet's top edge
- **AND** the selected source-list row uses compatible corner geometry so its visible top edge optically aligns with the sheet
- **AND** the selected source-list row remains quieter than an app-level sidebar tab and avoids button-like drop shadow
- **AND** alan does not present the internal Settings navigation as a second app-level sidebar, large tab bar, or stack of web buttons

### Requirement: Settings Rows Use Precise Native Form Rhythm
Alan macOS Settings rows SHALL use disciplined app-UI typography, stable columns,
and restrained dividers so settings can be scanned like a native form rather
than a dashboard card.

#### Scenario: Row columns align
- **WHEN** a selected Settings group renders multiple rows
- **THEN** icon, label, description, value, action, toggle, and segmented-control positions align consistently across rows in that group
- **AND** trailing controls share a stable right edge
- **AND** long trailing metadata values stay on one line with middle truncation and expose the full value through native help

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
- **AND** alan does not compensate for sparse content with oversized headers, hero spacing, or large decorative panels

### Requirement: Settings Surface Depth Avoids Web Dashboard Chrome
Alan macOS Settings SHALL use subtle native surface depth and restrained accent
color. It SHALL avoid visual treatments that make the surface read as a web
admin page.

#### Scenario: Surface layers are distinguishable
- **WHEN** Settings is visible in the default light appearance
- **THEN** the window/titlebar, Settings source list, detail pane, grouped rows, and controls are distinguishable through subtle material, tint, separators, and fill differences
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
- **THEN** maintainers can inspect a fresh Alan Dev light-mode screenshot showing source-list navigation, the selected General group, grouped rows, aligned trailing controls, and restrained accent color
- **AND** the screenshot is captured after relaunching Alan Dev rather than reusing a stale running window

#### Scenario: Visual review checks native criteria
- **WHEN** the screenshot is reviewed
- **THEN** maintainers compare it against this change's native-surface criteria: compact source list, stable white page sheet geometry, left-anchored grouped form content, stable row columns, subtle surface depth, no card-heavy dashboard chrome, and no oversized web-page spacing
