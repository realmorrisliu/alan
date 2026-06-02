## Context

`add-macos-settings-navigation` gives Settings a clearer information
architecture with General, Terminal, Agent, and System groups. The latest visual
review shows that the hierarchy is still not enough: the surface reads as a web
admin page because it uses a pale inner sidebar, a large white detail canvas,
and isolated card-like settings groups.

Alan's design context calls for a calm, precise, native macOS shell inspired by
Arc's material sidebar and terminal-first organization. Settings should support
that shell rather than becoming a dashboard, website page, or separate
preferences app. The visual reference for this pass is:

- Apple Settings for the native source-list plus grouped-form pattern.
- Linear for dense alignment, controlled row rhythm, and precise status columns.
- Notion only for concise secondary copy where a row needs context.

## Goals / Non-Goals

**Goals:**

- Make Settings feel native inside a macOS app rather than web-like.
- Preserve the current Settings group IA and row semantics.
- Turn the right detail area into a compact grouped settings form with clear
  content width, row rhythm, control alignment, and descriptions.
- Make the internal Settings navigation read as a subordinate macOS source list,
  not as a second application sidebar or button stack.
- Make the selected Settings group live in a stable white page sheet whose
  top border meets the pane title bar, and whose right and bottom edges stay
  8px from the pane edge, without an outer shadow.
- Keep the navigation-to-sheet seam tighter than the outer sheet margins so the
  source list and selected page feel connected inside one Settings surface.
- Keep the internal Settings source list close to the pane edges with compact
  leading, trailing, and top insets so the navigation reads as native chrome
  rather than a padded web sidebar.
- Align the first source-list row's visible top edge optically with the white
  page sheet's top edge, allowing a small compensation for rounded-corner
  antialiasing.
- Match the selected source-list row's visible corner treatment to the page
  sheet so the aligned top edges do not appear offset by antialiasing.
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

1. **Use Apple Settings as the primary structural pattern.**

   Settings should render as a source-list navigation plus an inset grouped
   form. The source list owns selection, and the detail side owns grouped rows.
   This is preferable to a web-dashboard layout because Settings is a
   configuration surface, not a page with cards.

   Alternative considered: keep the current two-column layout and only tune
   colors. That would leave the same web-like composition and would not fix the
   large white canvas or card-page reading.

2. **Use a stable page sheet with compact left-anchored content.**

   The selected group should render inside a white page sheet that is fixed to
   the detail pane with its top border flush against the pane title bar and 8px
   right and bottom insets. The sheet should avoid outer drop shadows and may
   use only a thin border plus focused inner edge treatment to focus the surface.
   The leading seam between the source list and sheet should be tighter than
   the right and bottom sheet margins, avoiding a visible dead gutter.
   Inside that sheet, rows keep a fixed maximum content width instead of
   stretching across the window. The form content sits near the upper-left with
   native preference spacing, so extra sheet space feels intentional rather than
   like an empty web canvas.

   Alternative considered: center the form in the available content area. That
   makes Settings feel like a web page and weakens alignment with the shell and
   source list.

3. **Replace card groups with inset grouped rows.**

   Section surfaces should use subtle grouped-list treatment: quiet fill,
   restrained stroke, row dividers aligned from the text column, and stable
   trailing controls. A settings group is not a dashboard card; it is a compact
   form section.

   Alternative considered: keep card containers and add shadows or stronger
   borders. That increases web chrome and fights the native material direction.

4. **Use Linear-like density and alignment, not Linear's product styling.**

   Rows should have fixed icon, text, value, and control columns. Labels,
   descriptions, values, toggles, and segmented controls should align across
   sections. The UI should become more precise through spacing, not through more
   decoration.

   Alternative considered: make rows larger and add more explanatory copy. That
   would make the surface feel heavier and less like a native developer tool.

5. **Treat the source list as pane chrome, not a page section.**

   The source list should use small edge insets and compact rows. The selected
   state may be visible, but the row should not look like a large floating
   card. Tight left, right, and top spacing keeps the navigation visually
   attached to the Settings pane title and page sheet. The first source-list
   row should optically align with the page sheet's top edge instead of
   floating lower in the rail; a small top compensation is acceptable when
   rounded-corner antialiasing makes strict geometric alignment look off. Its
   selected background should use compatible corner geometry with the page
   sheet so the visible antialiased edges read as aligned.

6. **Use Notion-like secondary copy only where it clarifies scope.**

   Short descriptions should explain what a preference affects, for example
   Sidebar or Inactive split dimming. Descriptions must be one concise line when
   possible and must not turn Settings into a documentation page.

   Alternative considered: omit descriptions to keep the UI minimal. The recent
   screenshots show that this makes sparse groups feel empty and lowers
   confidence in what each setting controls.

7. **Tone down accent color dominance.**

   Accent blue should identify selected controls and active state, not define
   the whole page. Native controls should be preferred over custom bright pills
   where possible, and selected navigation should use source-list treatment
   rather than a large white button.

   Alternative considered: use accent color to make the page feel more alive.
   That solves the wrong problem; the current surface needs hierarchy and native
   rhythm, not stronger color.

8. **Verify with a fresh Alan Dev launch and screenshot review.**

   Unit tests can prove row membership and bindings, but they cannot prove that
   Settings no longer looks like a web app. This change requires a fresh Alan
   Dev launch, light-mode screenshot review, and explicit comparison against the
   native-surface requirements.

   Alternative considered: rely on Swift tests and `xcodebuild` only. That
   misses the primary failure mode reported by the user: visual quality.

## Risks / Trade-offs

- [Risk] Native grouped forms become too similar to Apple System Settings. ->
  Keep Alan-specific shell context, compact row density, and Arc-like material
  continuity with the outer sidebar.
- [Risk] More row descriptions reduce density. -> Keep descriptions short and
  optional; use them where a setting name alone is ambiguous.
- [Risk] Tightening width makes long Agent or System values truncate. -> Use
  stable trailing value behavior, tooltips or reveal actions for paths, and
  existing unavailable/detail rows instead of stretching the full page.
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
4. Update the selected group detail pane to an inset grouped-form layout.
5. Add or adjust concise row descriptions for ambiguous General rows first, then
   extend only where Terminal, Agent, or System rows need scope clarity.
6. Run focused Swift/script tests and a macOS build from repo-local DerivedData.
7. Launch a fresh Alan Dev build and verify Settings visually in light mode.

Rollback: revert the visual component changes while keeping the existing
Settings navigation model and row data intact. The IA change remains valid even
if this visual polish is backed out.

## Open Questions

- None for this polish slice.
