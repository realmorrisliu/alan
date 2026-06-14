## MODIFIED Requirements

### Requirement: Root shell backing uses a unified paper base
(Renamed and modified from "Root shell backing uses an opaque native base" in
the pending `polish-macos-workspace-colors` change, which this change
supersedes for the light-appearance scenario.)

The default macOS shell SHALL paint its primary root backing as one
continuous paper material surface shared by the sidebar column and the
workspace margins, and SHALL reserve white for raised surfaces above that
base.

#### Scenario: Light appearance root backing
- **WHEN** the default macOS shell window is visible in light appearance
- **THEN** the root backing renders the unified sidebar-material treatment
  (visual effect material plus cool scrim) across the whole window chrome
- **AND** the sidebar column and the margins around the workspace panel read
  as one continuous surface without a vertical seam
- **AND** the raised workspace panel uses the white raised paper fill above
  that base

#### Scenario: Dark appearance root backing
- **WHEN** the default macOS shell window is visible in dark appearance
- **THEN** the root backing uses the dark paper material treatment and sits
  below the terminal ink surface in relative luminance

#### Scenario: Reduced transparency
- **WHEN** reduce transparency is enabled
- **THEN** the root backing falls back to the opaque window paper fill
  without wallpaper dependence, and the chrome remains one continuous surface
