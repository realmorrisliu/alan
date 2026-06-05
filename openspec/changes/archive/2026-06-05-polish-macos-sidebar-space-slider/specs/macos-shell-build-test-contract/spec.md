## ADDED Requirements

### Requirement: Sidebar Space slider layout has focused verification
The Apple client SHALL include focused automated or documented verification for
the top Space slider layout, titlebar New Space control, Space context menu
profile disclosure, and New Tab placement when the sidebar layout changes.

#### Scenario: Sidebar layout ordering is verified
- **WHEN** the sidebar Space slider layout is implemented
- **THEN** focused checks verify that the default sidebar orders controls as
  titlebar controls, top Space slider, pinned tabs, pinned-tab divider when
  pinned tabs exist, New Tab row, and unpinned tabs
- **AND** focused checks verify that titlebar New Space is right-aligned within
  the sidebar titlebar while pin/unpin and appearance controls remain leading
- **AND** focused checks verify that the bottom Space dock and always-visible
  Space profile selector are absent from the default sidebar
- **AND** focused checks verify that the Space slider is fixed outside the
  per-Space content pager instead of being rendered inside each Space page

#### Scenario: Sidebar interaction surfaces are verified
- **WHEN** Space slider title or dot controls, the titlebar New Space button,
  the New Tab row, or tab rows are changed
- **THEN** focused window-placement checks verify those controls remain
  interaction surfaces rather than hidden-titlebar window-drag surfaces
- **AND** existing empty chrome double-click zoom behavior remains covered for
  non-control titlebar/sidebar chrome

#### Scenario: Visual evidence captures the new sidebar hierarchy
- **WHEN** sidebar Space slider polish is marked complete
- **THEN** maintainers can inspect running-app screenshots or manual notes from
  a fresh Alan Dev launch showing the light-mode pinned sidebar with the top
  Space slider, titlebar New Space control, pinned-tab divider behavior, New Tab
  row, unpinned tabs, and no bottom Space dock
- **AND** if collapsed-sidebar rendering is affected, visual evidence also
  covers the collapsed floating sidebar reveal
