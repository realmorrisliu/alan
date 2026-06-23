## Why

Groove Master is a real bass practice app for developing groove, pocket, and
personal musical feel. It should not be reduced to a demo for Alan, a generic
music education feature, or a gamified instrument trainer.

The product belongs naturally inside Alan's future programmable environment
because it is a long-lived creative workflow: a user follows practice plans,
plays with drum loops, records sessions, marks moments, reflects, and builds a
local journal of creative material over time. Humans and agents can participate
through the same objects, commands, buffers, views, queries, and local-first
state model that the programmable environment is meant to provide.

The first design should define Groove Master as an environment app with a narrow
daily-use V1: open today's plan, start a loop, record the room, mark moments,
reflect, and save the session into a Groove Journal.

## What Changes

- Define Groove Master as a creative practice app that lives inside the future
  Alan programmable environment while remaining portable beyond macOS.
- Define the V1 daily practice loop: today plan, live session, drum loop,
  room-capture recording, markers, end reflection, and Groove Journal entry.
- Define the practice model as a concrete 12-month route with fixed spine,
  weekly cadence, daily practice blocks, and adaptive micro-adjustment.
- Define metronome, drum loop, and inspiration reference as distinct session
  sources.
- Define a progressive audio model that starts with room capture and keeps the
  journal schema open for clean input and multitrack capture later.
- Define drum loops as built-in pack plus imported local loop folders.
- Define Inspiration Cards as musical references for exploration rather than
  forced repertoire.
- Define Pocket Tracker as non-graded reflection over continuous play time,
  space, and consistency.
- Define the producer agent as a low-presence planner, reflector, and curator
  rather than a teacher, grader, or real-time correction engine.
- Define how Groove Master maps into Alan environment abstractions without
  coupling the domain core to the current macOS shell implementation.
- Decompose follow-up implementation slices instead of building the full
  product in one change.

## Programmable Environment Alignment

Groove Master is an **environment app** in the programmable environment model. Its
domain core owns music-practice truth: phases, plans, practice blocks, loop metadata,
recording references, markers, reflections, pocket-tracker signals, journal entries,
and producer notes. Alan integration is an adapter that exposes that domain through
environment objects, commands, buffers, views, queries, local-first persistence, and
agent participation. The macOS surface is the first host, not the domain authority,
and the current Alan shell must not leak into the portable Groove Master core.

## Capabilities

### New Capabilities

- `groove-master-environment-app`: Defines Groove Master's product boundary,
  V1 practice loop, practice plan model, audio and journal model, drum loop
  library, producer agent role, Alan environment integration, and deferred
  capabilities.

### Modified Capabilities

- None. This change intentionally does not modify current Alan runtime, daemon,
  terminal, macOS shell, or programmable-environment constitution behavior.

## Impact

- Affected product planning: introduces Groove Master as a future environment
  app inside the Alan repo.
- Affected future architecture planning: later implementation changes should
  preserve separate Groove Master domain core, audio runtime, Alan environment
  adapter, and macOS surface boundaries.
- Affected current code: none in this change.
- Affected current Alan shell behavior: none.
