## ADDED Requirements

### Requirement: Adaptive Space slider polish has focused verification
The Apple client SHALL include focused automated checks or documented visual
verification for adaptive Space slider density, hover preview, scrub
preview/commit behavior, input protection, accessibility, and hit-test
preservation.

#### Scenario: Density tiers are verified
- **WHEN** the adaptive Space slider is implemented
- **THEN** focused checks verify the 1-3, 4-6, and 7-9 Space density tiers
- **AND** focused checks verify that the Space cap is 9 and creation affordances
  do not expose a tenth default-sidebar Space

#### Scenario: Static layout stability is verified
- **WHEN** Space titles are long, localized, or mixed with different Space counts
- **THEN** focused checks or documented review verify that the slider remains
  single-line, does not resize the sidebar, keeps the fixed top slider position,
  and truncates inactive titles before text overlaps adjacent controls

#### Scenario: Hover and click behavior is verified
- **WHEN** Space slider hover or click behavior changes
- **THEN** focused checks verify hover preview does not switch Spaces
- **AND** focused checks verify clicking a non-selected Space switches
  immediately while clicking the selected Space leaves selection unchanged

#### Scenario: Scrub preview and commit are verified
- **WHEN** Space slider scrub behavior is implemented
- **THEN** focused checks verify press-drag scrub moves preview focus before
  commit
- **AND** focused checks verify horizontal wheel or trackpad scrub commits only
  after release or dwell timing rather than switching on every delta
- **AND** focused checks verify canceling scrub restores the previous selected
  Space when no commit occurred

#### Scenario: Scroll input protection is verified
- **WHEN** wheel or trackpad routing for the Space slider changes
- **THEN** focused checks verify clear horizontal input can enter scrub
- **AND** vertical or ambiguous input does not prevent the tab list from
  scrolling vertically

#### Scenario: Accessibility and reduced motion are verified
- **WHEN** adaptive Space slider polish is marked complete
- **THEN** focused checks or documented review verify VoiceOver labels,
  selected-state announcements, keyboard preview/commit/cancel behavior, and
  reduced-motion scrub behavior

#### Scenario: Hidden-titlebar hit testing remains verified
- **WHEN** slider hit areas, hover expansion, or scrub controls are changed
- **THEN** focused window-placement checks verify Space slider controls are not
  treated as blank double-click zoom chrome
- **AND** blank sidebar chrome outside actual controls remains available for
  hidden-titlebar double-click zoom

#### Scenario: Visual evidence captures all density tiers
- **WHEN** adaptive Space slider implementation is ready for review
- **THEN** maintainers can inspect running-app screenshots or notes from a fresh
  Alan Dev launch showing the light-mode sidebar at representative 1-3, 4-6,
  and 7-9 Space counts
- **AND** visual evidence covers hover expansion, scrub preview focus,
  post-commit selected state, and the absence of the removed bottom Space dock
