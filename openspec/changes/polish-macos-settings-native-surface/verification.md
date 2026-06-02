## Visual Verification

- Fresh Alan Dev build launched:
  `debug/DerivedData/install-dev-polish-settings-native-arm64/Build/Products/Release/Alan Dev.app`
- Captured screenshot:
  `debug/alan-dev-settings-fixed-sheet-polish.png`
- Reviewed state:
  Settings in light mode, General selected, Alan Dev only.

## Result

The screenshot passes the updated native-surface checklist:

- Settings title, navigation rail, and pane backdrop share a shallow gray plane.
- The selected group sits in a wider white page sheet.
- The white sheet's top border meets the pane title bar bottom edge.
- The white sheet keeps stable 8px right and bottom insets from the Settings pane.
- The white sheet uses a restrained 8pt corner radius so the sheet edge does not fight the pane edge.
- The white sheet has no outer drop shadow.
- The sheet uses a thin border and focused inner edge treatment for focus.
- General row content remains left anchored inside the sheet and does not stretch across the full sheet.
- Row labels, descriptions, segmented control, and toggles keep stable alignment.
- The surface avoids dashboard cards, decorative gradients, and web-page hero spacing.

The visual pass focused on the sheet geometry and surface hierarchy using the
General group. Script tests cover the other Settings groups' row membership and
model semantics.
