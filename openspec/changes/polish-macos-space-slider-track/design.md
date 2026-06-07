## Context

The current macOS Space slider was recently shaped around density tiers:
1-3 Spaces show full titles, 4-6 Spaces show short titles, 7-9 Spaces collapse
inactive Spaces to indicators, and the model caps visible Spaces at nine. It
also uses hover and scrub states to change item width, scale, and opacity.

The new direction is closer to Safari's tab strip: one continuous rounded track,
with the selected Space represented by a liquid-glass tab on that track and
inactive Spaces sitting quietly in the shared background. The user also wants
Space icons to remain supported, with the minimum Space tab becoming a circular
icon-only target when there is not enough width.

Existing `ShellSpace` state only carries ID, title, attention, tabs, selected
Tab, and terminal profile binding. Since the slider should render Space icons
consistently across launches, this change needs a small presentation-metadata
extension instead of deriving icons from Terminal Profiles.

## Goals / Non-Goals

**Goals:**

- Replace the tiered pill/dot slider with a continuous rounded track.
- Render the selected Space as a liquid-glass tab inside the track.
- Keep inactive Spaces embedded in the track background without individual pill
  cards or notification-dot styling.
- Show `icon + title` when width allows, then truncated title, then icon-only
  circular targets at the minimum width.
- Distribute Space targets evenly across the full track until that equal share
  would fall below the minimum target width.
- Remove the nine-Space cap; all Spaces participate in the slider.
- Horizontally scroll the track when icon-only targets still overflow the
  sidebar width.
- Remove hover-driven width, scale, and opacity changes so pointer movement does
  not make the slider feel jumpy.
- Preserve click selection, context menus, keyboard navigation, drag scrub,
  wheel scrub, and vertical-scroll pass-through semantics.
- Persist optional Space icon metadata with backwards-compatible defaults.

**Non-Goals:**

- No Space icon picker, custom icon management UI, or Arc-style page
  customization menu in this change.
- No bottom Space dock, two-column sidebar, or broader Space management
  redesign.
- No daemon API, runtime session API, provider configuration, or terminal
  runtime changes.
- No dark-mode redesign beyond preserving adaptive material behavior.
- No changes to active-Space tab row behavior beyond layout spacing needed to
  integrate the new slider.

## Decisions

### Use a continuous track as the slider's only background

The Space slider should render a single rounded track spanning the sidebar's
content column. Inactive Spaces should not draw their own persistent capsule,
card, shadow, or dot background. The selected Space is the only item with a
strong surface, and that surface sits inside the track.

The track owns hit testing and horizontal scrolling. It should stay aligned to
the sidebar row inset so the slider and tab rows share one optical column. The
track height should be stable across hover, selection, scrub, and Space count.

### Treat Liquid Glass as selected-state material, not decoration

The selected Space tab should use the native Liquid Glass treatment where the
deployment target supports it. The selected tab remains compact, with title and
icon content on top of the material. If a local SDK or OS path cannot provide
Liquid Glass, the implementation may fall back to Alan's existing adaptive
material token while preserving the same geometry, contrast, and selected-state
hierarchy.

Inactive Spaces should use text/icon foreground treatment only. Hover and
keyboard focus may adjust foreground, tint, or a subtle outline, but they must
not introduce independent framed pills or resize the item.

### Replace density tiers with width allocation

The layout model should stop using fixed count buckets, fixed maximum Space
target widths, and `maximumVisibleSpaces`. All Spaces should be measured and
included. Width allocation should divide the available rounded track evenly
across every Space until that equal share would drop below the stable minimum
target width.

The collapse path is:

1. `icon + full title` when the equal per-Space width can support it.
2. `icon + truncated title` when the equal per-Space width has partial room.
3. `icon-only` when the equal per-Space width is above the minimum but cannot
   support a title.
4. Minimum-width icon-only targets with horizontal overflow when equal division
   would fall below the minimum width.

If all items are already at their minimum circular width and still exceed the
available track width, the track scrolls horizontally instead of hiding Spaces
or reintroducing an arbitrary cap.

### Keep hover visually quiet

Hover should only identify the target under the pointer. It should not expand
the target, scale it, fade neighbors, or change the selected Space. Removing
hover geometry shifts is the main reason this design should feel closer to
Safari and less like a cover-flow control.

### Preserve scrub without cover-flow motion

Drag and wheel scrub remain useful for fast Space switching, but the scrub
preview should be expressed through the same track language: selected tab,
scrub-focused foreground, and optional subtle focus treatment. The old
cover-flow-style scale and fade behavior should be removed.

Scrub coordinate mapping must account for horizontal scroll offset. Vertical or
ambiguous wheel/trackpad input over the slider must still pass through to normal
sidebar scrolling instead of starting a horizontal scrub.

### Persist Space icons as presentation metadata

Add optional Space icon metadata to the workspace manifest Space record and
`ShellSpace` projection. The value should be a stable SF Symbol name or the
project's existing symbol token type if one is already available locally.

Existing manifests without the icon field must decode successfully. When absent
or invalid, alan should use a deterministic default Space icon for display
without rewriting Terminal Profile definitions or treating the icon as profile
metadata. This keeps profile icon ownership narrow and makes Space slider icon
rendering stable across restarts.

### Verify with layout, contract, and Alan Dev visual checks

Implementation should include focused layout tests for no max cap,
icon-only collapse, overflow scroll sizing, selected-item visibility, and hover
geometry stability. Contract checks should reject the old nine-Space cap and
hover scale/fade model.

Visual verification should use Alan Dev only, with a fresh relaunch before
screenshots. The key states are one Space, several readable Spaces, many
icon-only Spaces with horizontal overflow, selected liquid-glass tab, hover
without geometry shift, and scrub preview without cover-flow motion.

## Risks

- Liquid Glass over the existing sidebar material may be too bright or too low
  contrast in light mode. Mitigate with adaptive tokens and screenshot review.
- Horizontal scrolling can conflict with scrub if coordinate mapping ignores the
  scroll offset. Mitigate with layout tests and focused wheel/scrub tests.
- Icon-only overflow can make many Spaces harder to identify. Mitigate by
  keeping the selected item visible, preserving accessibility labels, and
  allowing title labels whenever width permits.
- Adding icon metadata touches persistence. Keep the field optional and defaulted
  so old manifests remain valid and no migration prompt is needed.
