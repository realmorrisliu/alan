## Context

The active `polish-macos-sidebar-space-slider` change moves Space navigation
into a fixed top slider, removes the bottom Space dock, moves New Space into the
sidebar titlebar, and places New Tab between pinned and unpinned tabs. That
first pass solves layout structure, but the slider still needs a more polished
visual and motion model.

The target interaction should feel closer to native Safari tab density at low
Space counts and closer to Arc-like compact navigation at high Space counts.
It must also support fast Space selection without making the default sidebar
visually noisy or causing the tab pager to switch repeatedly during rapid input.

## Goals / Non-Goals

**Goals:**

- Support up to 9 Spaces in the default sidebar.
- Use three density tiers for the Space slider: `1-3`, `4-6`, and `7-9`.
- Keep low Space counts readable with named tab-like controls.
- Keep high Space counts quiet with active-title plus indicator controls.
- Add hover preview that locally expands or highlights the target Space without
  switching.
- Add scrub preview through both horizontal wheel/trackpad input and press-drag
  input.
- Commit scrub selection after release or a short focus dwell so the tab pager
  does not churn during fast scrubbing.
- Preserve Space context menus, click-to-switch behavior, hidden-titlebar
  hit-testing, keyboard access, reduced motion, and VoiceOver semantics.

**Non-Goals:**

- Add Space overflow beyond 9 Spaces.
- Add 3D transforms, perspective, or large cover-flow surfaces.
- Move New Space back into the slider row.
- Change tab organization, pinned tab semantics, terminal profile storage, or
  daemon/runtime APIs.
- Introduce a generalized carousel component outside the shell sidebar.

## Decisions

### Decision: Use adaptive density tiers instead of one fixed slider layout

The Space slider uses three fixed tiers:

- `1-3`: all Spaces show full titles in quiet Safari-like tab/pill controls.
- `4-6`: active Space shows its full title; inactive Spaces show short,
  single-line truncated titles in narrower tab/pill controls.
- `7-9`: active Space shows its title; inactive Spaces show compact indicator
  dots that can expand while hovered or scrub-focused.

This makes the control useful when only a few Spaces exist while keeping the
full 9-Space state calm and scannable.

Alternative considered: always render active title plus dots. That is compact,
but wastes available space and hides useful names when users only have a few
Spaces. Another alternative was to keep Safari-like tabs at all counts, but
that becomes crowded and visually heavy in the sidebar at 7-9 Spaces.

### Decision: Keep static layout stable and bounded

The slider stays inside the sidebar edge insets, remains single-line, and does
not resize the sidebar or push titlebar controls. Long titles truncate before
they cause layout growth. The active Space receives the most readable width;
inactive titles truncate earlier.

The active item is not forcibly centered in static mode. This avoids large
position shifts when clicking nearby Spaces and keeps the top of the sidebar
quiet during ordinary tab work.

### Decision: Separate hover preview from selection

Hovering a Space target only changes local presentation:

- In `1-3`, the hovered inactive tab becomes slightly brighter and text
  strengthens.
- In `4-6`, the hovered inactive tab can widen modestly to show more title.
- In `7-9`, the hovered indicator expands into a short-title pill, with nearby
  indicators subtly emphasizing proximity.

Hover never changes the active Space or moves the tab pager. This keeps pointer
exploration cheap and avoids surprising workspace changes.

### Decision: Use scrub preview with delayed commit

The slider supports two scrub inputs:

- Horizontal wheel or trackpad movement while the pointer is over the slider.
- Press-drag movement that crosses a horizontal threshold.

Entering scrub mode creates a temporary preview focus. The focused Space becomes
the largest title pill in a lightweight cover-flow-like rail; neighboring Spaces
scale and fade by distance. The currently active Space keeps a selected marker
when it differs from the scrub focus, so preview and selection remain distinct.

Scrub does not immediately switch the tab pager. Selection commits when the user
releases the drag or when wheel/trackpad focus dwells for roughly `120-180ms`.
After commit, the existing tab pager transition handles the content switch and
the slider springs back to its static tier layout.

Alternative considered: immediate switching while scrubbing. That feels direct
but can churn the sidebar content and make the terminal workflow feel unstable
with fast trackpad input.

### Decision: Protect vertical scrolling and hidden-titlebar behavior

Wheel/trackpad input only enters scrub when horizontal intent is clear. Vertical
or ambiguous scroll input continues to the tab list and must not be captured by
the slider.

The slider must continue to participate in the hidden-titlebar hit-test model as
an explicit control region, while blank sidebar chrome remains available for
double-click zoom. Scrub hit areas should be broad enough for usability without
turning the whole titlebar or spacer region into a control.

### Decision: Keep accessibility and reduced motion first-class

Each Space remains an individual accessibility button with label data for title,
selected state, and tab count. Keyboard focus enters the slider as a group;
left/right arrows move preview focus, Enter commits, and Escape cancels preview.

When reduced motion is enabled, scrub avoids cover-flow scale and springy
movement. It keeps the same state model using highlight, opacity, and bounded
width changes.

## Risks / Trade-offs

- [Risk] The scrub interaction can steal vertical tab-list scrolling.
  -> Mitigation: require clear horizontal intent before entering scrub and keep
  ambiguous wheel input pass-through.

- [Risk] Cover-flow motion can feel decorative or distracting in a terminal
  product.
  -> Mitigation: make the effect lightweight, sidebar-local, and only active
  during deliberate scrub input.

- [Risk] Multiple density tiers can make tests and layout math brittle.
  -> Mitigation: extract a small layout model with deterministic inputs for
  Space count, selected index, hovered index, scrub focus, available width, and
  reduced-motion state.

- [Risk] Hover expansion can cause distracting row shifts.
  -> Mitigation: keep expansion bounded within the slider width, avoid changing
  slider height, and use predictable truncation.

- [Risk] The 9-Space cap changes previous assumptions from the first-pass
  slider work.
  -> Mitigation: update UI contracts, shell contract checks, and creation guard
  tests in the same change.

## Migration Plan

Implement after `polish-macos-sidebar-space-slider` is merged or otherwise
stabilized, because this change assumes the fixed top slider and removed bottom
Space dock from that baseline. If the scrub polish is reverted, alan can fall
back to the first-pass static slider without affecting persisted workspace data.
