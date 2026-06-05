## Context

`add-macos-settings-navigation` gives Settings a clearer information
architecture with General, Terminal, Agent, and System groups. The latest visual
review shows that the hierarchy is still not enough: the surface reads either
as a web admin page or as an Apple System Settings clone because it relies on a
large white sheet, low information density, and weak preference-list rhythm.

Alan's design context calls for a calm, precise, native macOS shell inspired by
Arc's material sidebar and terminal-first organization. Settings should support
that shell rather than becoming a dashboard, website page, or separate
preferences app. The visual reference for this pass is:

- Linear for dense alignment, controlled row rhythm, and precise status columns.
- Raycast and Warp for developer-tool settings that scan like a control panel.
- Apple Pro Apps for native control-panel discipline without heavy cards.

## Goals / Non-Goals

**Goals:**

- Make Settings feel native inside a macOS app rather than web-like.
- Preserve the current Settings group IA and row semantics.
- Turn the right detail area into a compact preference list with clear content
  width, row rhythm, control alignment, and descriptions.
- Make the internal Settings navigation read as a subordinate macOS source list,
  not as a second application sidebar or button stack.
- Remove the stable white page sheet as the main visual device; use the shared
  pane plane plus direct section dividers so Settings reads as a control panel
  rather than a document page.
- Keep the navigation-to-detail seam tight so the source list and selected group
  feel connected inside one Settings surface.
- Keep the internal Settings source list close to the pane edges with compact
  leading, trailing, and top insets so the navigation reads as native chrome
  rather than a padded web sidebar.
- Align the first source-list row's visible top edge optically with the detail
  content start, allowing a small compensation for rounded-corner antialiasing.
- Reduce excessive white space, heavy card affordances, and accent-color
  dominance.
- Add verification tasks that require a fresh Alan Dev visual check.

**Non-Goals:**

- Changing Settings group membership, row ownership, redaction rules, or
  unavailable-state semantics.
- Adding new user settings or making read-only rows editable.
- Moving Settings to a separate macOS `Settings` scene or preferences window.
- Replacing Alan's outer shell sidebar, tab model, or content-container
  behavior.
- Designing dark mode for Settings in this change.

## Decisions

1. **Use a developer control-panel pattern, not Apple System Settings.**

   Settings should render as source-list navigation plus a dense preference
   list. The source list owns selection, and the detail side owns sectioned
   label/value rows. This is preferable to a web-dashboard layout or Apple
   System Settings clone because Alan is a developer tool, and Settings should
   behave like a control panel for fast scanning and modification.

   Alternative considered: keep the current two-column layout and only tune
   colors. That would leave the same web-like composition and would not fix the
   large white canvas or card-page reading.

2. **Use direct sections inside a stable 760pt content column.**

   The selected group should render directly on the detail pane with a stable
   maximum content width of roughly 760pt, left-anchored placement within the
   detail pane, and section dividers. Removing the white sheet avoids the document-page
   feeling and lets hierarchy come from typography, separators, and row
   alignment instead of container chrome. Rows should not stretch across the
   full window, but the content width should be broad enough for paths,
   endpoints, and metadata to scan quickly. The detail content should use a
   predictable top offset and balanced horizontal padding so wide windows do
   not make the content drift right.

   Alternative considered: center the form in the available content area. That
   makes Settings feel like a web page and weakens alignment with the shell and
   source list.

3. **Replace card groups with section dividers and compact rows.**

   Section surfaces should use a readable title, a quiet horizontal divider,
   row dividers aligned from the text column, and stable trailing controls. A
   settings group is not a dashboard card; it is a compact preference section.

   Alternative considered: keep card containers and add shadows or stronger
   borders. That increases web chrome and fights the native material direction.

4. **Use one native setting-row template, not a database table.**

   Rows should share one structure: setting title, optional secondary detail,
   and an optional native trailing control. Read-only System metadata with no
   control should render the value as secondary text below the label, not as a
   far-right table cell. Rows with controls, such as toggles, segmented controls,
   Copy, Show..., and Export, should align to a bounded trailing control column.
   The UI should become more precise through spacing, not through more
   decoration.

   Typography should use native macOS system text roles rather than page-like
   web hierarchy. Source-list labels, section labels, row labels, descriptions,
   and trailing values should each have distinct size, weight, and blue-gray ink
   roles. Row values should read as metadata, not competing primary labels, and
   descriptions should be clearly secondary without becoming low-contrast.
   Row labels should use restrained native weight so the form feels like a
   settings surface rather than a table of headings. Long metadata values such
   as paths should stay subordinate as secondary text while preserving the full
   value through native help or a real action.

   Alternative considered: make rows larger and add more explanatory copy. That
   would make the surface feel heavier and less like a native developer tool.

5. **Make System rows behave like a control panel.**

   System should not read like an About page or `select * from settings`.
   Read-only install facts such as Bundle ID, Channel, and Updates remain direct
   title/value rows, with the value below the label. Rows with obvious local
   actions should expose compact native affordances: Daemon Endpoint can be
   copied, Shell state and Alan home can be shown in Finder, and Diagnostics
   remains a toggle plus export action. Update explanations and path
   implementation details should move out of always-visible copy unless they
   are needed for current user action.

   Alternative considered: make Channel a disabled dropdown to add visual
   activity. That would create a fake control and reduce trust.

6. **Treat the source list as pane chrome, not a page section.**

   The source list should contain only the four navigation rows, without an
   internal `Settings` title. The pane titlebar and selected detail page already
   name the surface, so another title in the navigation rail creates duplicate
   hierarchy. The selected state should use a macOS-style rounded capsule fill
   with darker active text and icon treatment, without a blue accent bar. The
   navigation list should start 24pt below the Settings content top, with 12pt
   leading inset and 8pt trailing inset so the rail has enough air without
   becoming a second sidebar.

   Source-list labels should stay lighter than the outer application sidebar:
   icons around 13pt, labels around 13pt, and selected emphasis expressed
   through primary text color rather than blue labels.

7. **Use native action language.**

   Action labels should read like a macOS app. Folder actions should use a
   compact native button labeled `Show...`; deferred commands should use an
   ellipsis, such as `Create...` or `Preview...`; external-link arrows should
   not appear in native Settings rows. The daemon endpoint should use a real
   `Copy` control rather than blue link styling.

8. **Use secondary copy only where it clarifies scope.**

   Short descriptions should explain what a preference affects, for example
   Sidebar or Inactive split dimming. Descriptions must be one concise line when
   possible and must not turn Settings into a documentation page.

   Alternative considered: omit descriptions to keep the UI minimal. The recent
   screenshots show that this makes sparse groups feel empty and lowers
   confidence in what each setting controls.

9. **Tone down accent color dominance.**

   Accent blue should identify selected controls and active state, not define
   the whole page. Native controls should be preferred over custom bright pills
   where possible, and selected navigation should use source-list treatment
   rather than a large white button.

   Alternative considered: use accent color to make the page feel more alive.
   That solves the wrong problem; the current surface needs hierarchy and native
   rhythm, not stronger color.

10. **Verify with a fresh Alan Dev launch and screenshot review.**

   Unit tests can prove row membership and bindings, but they cannot prove that
   Settings no longer looks like a web app. This change requires a fresh Alan
   Dev launch, light-mode screenshot review, and explicit comparison against the
   native-surface requirements.

   Alternative considered: rely on Swift tests and `xcodebuild` only. That
   misses the primary failure mode reported by the user: visual quality.

## Risks / Trade-offs

- [Risk] Preference lists become too plain. -> Keep Alan-specific shell context,
  precise typography, compact row density, and subtle pane material continuity
  instead of adding cards or large sheets.
- [Risk] More row descriptions reduce density. -> Keep descriptions short and
  optional; use them where a setting name alone is ambiguous.
- [Risk] Tightening width makes long Agent or System values truncate. -> Use
  stable value behavior, native help, and explicit copy/open actions for
  endpoints and paths instead of stretching the full page.
- [Risk] Visual polish touches shared row components and regresses behavior. ->
  Preserve the settings surface model and keep focused tests for bindings,
  navigation, redaction, and unavailable states.
- [Risk] Screenshot verification becomes subjective. -> Pair screenshot review
  with concrete checks: source-list selection, grouped-row treatment, compact
  content width, aligned controls, toned-down accent use, and no card-heavy
  dashboard composition.

## Migration Plan

1. Audit the current Settings view components and identify which styles are
   shared by navigation, sections, rows, values, and controls.
2. Refactor presentation tokens for Settings-specific surfaces, typography,
   row dimensions, and separators without changing row data semantics.
3. Update the internal navigation to a native source-list treatment.
4. Update the selected group detail pane to a direct sectioned preference-list
   layout.
5. Collapse row presentation into one title/detail/control template, keeping
   read-only values subordinate and controls bounded on the trailing edge.
6. Add compact copy/open/export affordances for System rows that already have a
   natural local action, without implying read-only install facts are editable.
7. Add or adjust concise row descriptions for ambiguous General rows first, then
   extend only where Terminal, Agent, or System rows need scope clarity.
8. Run focused Swift/script tests and a macOS build from repo-local DerivedData.
9. Launch a fresh Alan Dev build and verify Settings visually in light mode.

Rollback: revert the visual component changes while keeping the existing
Settings navigation model and row data intact. The IA change remains valid even
if this visual polish is backed out.

## Open Questions

- None for this polish slice.
