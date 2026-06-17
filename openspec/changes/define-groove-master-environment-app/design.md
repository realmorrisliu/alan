## Context

Groove Master is a bass practice app for feeling groove rather than completing
lessons. The user's real goal is to open the app, follow a practice plan, play
with a drum loop, record the session, and gradually build a personal archive of
grooves. It is not a music-school app, a score-chasing trainer, or a Guitar
Hero-like evaluation surface.

The product can live inside Alan's future programmable environment because it
is an object-rich creative workflow:

```text
PracticePlan
  -> GrooveSession
  -> RecordingTake + Markers
  -> Reflection + ProducerNote
  -> GrooveEntry
  -> GrooveStream
```

Alan provides the environment substrate: objects, commands, buffers, views,
queries, permissions, local-first persistence, and agent participation. Groove
Master owns the music-practice logic and user experience. The first design
therefore defines Groove Master as a serious environment app, while keeping the
first implementation slice small enough to become a daily-use tool.

## Goals / Non-Goals

**Goals:**

- Define Groove Master as a real bass practice app inside the future Alan
  programmable environment.
- Preserve the core product promise: practice plan, drum loop, recording,
  reflection, and journal.
- Define V1 as one complete daily practice loop rather than a broad music
  platform.
- Define the practice plan as fixed spine plus adaptive micro-adjustment.
- Start audio with low-friction room capture while preserving a schema path for
  clean bass capture and multitrack sessions later.
- Treat recordings as memory, reflection, and creative material rather than
  grading evidence.
- Define a low-presence producer agent that plans, reflects, and curates.
- Keep the domain core portable so iOS and iPadOS remain viable later.

**Non-Goals:**

- Real-time timing correction or automatic scoring.
- Accuracy percentages, ranks, combo systems, XP, or gamified streak pressure.
- Song tutorials or forced repertoire.
- Full music theory, scales, chords, or modes in the first slice.
- Multitrack editing in V1.
- A professional DAW, loop marketplace, collaboration platform, or cloud sync
  system.
- A macOS-only domain model that cannot later support iPhone or iPad capture.
- A demo whose primary purpose is to prove Alan.

## Decisions

### Define Groove Master As An Environment App, Not A Demo

Groove Master should be designed for real daily use. Alan's programmable
environment is the host model, not the reason the product exists.

The app maps naturally onto the environment abstractions:

```text
Objects:   PracticePlan, GrooveSession, RecordingTake, Marker, Clip,
           GrooveEntry, DrumLoop, Reflection, ProducerNote
Commands:  start session, mark moment, end session, replay take,
           import loops, generate tomorrow plan
Buffers:   active session, recording review, groove journal entry,
           loop library
Views:     today view, session view, groove stream, recording detail,
           loop browser
Queries:   recent marked moments, sessions by phase, loops by style,
           entries needing reflection
Agent:     low-presence producer
```

Alternative considered: treat Groove Master as a proof-of-concept for the
programmable environment. That reverses the product priority and risks building
a demo instead of a practice tool.

### Use A Complete Daily Practice Loop For V1

V1 should be the smallest useful loop:

```text
Today Plan
  -> Live Session
  -> Marker capture
  -> End Reflection
  -> Groove Entry
```

The Today Plan shows the current phase, focus, session length, selected drum
loop, and one challenge. The Live Session starts playback, timer, room capture,
and optional micro-prompts. During practice, the main action is a marker command
for moments worth revisiting. The session ends with a short reflection and a
saved Groove Entry.

This gives the user something they can follow every day without requiring a
large loop editor, lesson catalog, or audio-analysis stack.

### Use Fixed Spine Plus Adaptive Micro-Adjustment

The practice plan needs enough structure to feel trustworthy:

```text
Groove -> Pocket -> Jam -> Language -> Vocabulary
```

Each phase has a clear intention:

- Groove: one note, pulse, repetition, breathing.
- Pocket: silence, space, root/rest, root/octave.
- Jam: longer playing, loop expansion/reduction, flow.
- Language: name what the user already feels.
- Vocabulary: harmony, chords, scales, modes, song analysis.

The adaptive layer should modify daily details, not replace the spine. It can
choose loop, session length, micro-prompt, review target, continuation from a
prior groove, and whether to stay in a phase longer. V1 adaptation should use
simple durable signals: completed sessions, uninterrupted play time, markers,
reflection tags, selected loop style, skipped days, and producer notes.

Alternative considered: a fully adaptive plan. That is attractive later, but it
would overfit before the app has enough personal history and would make the
learning path feel less grounded.

### Start With Room Capture But Keep Audio Progressive

V1 should default to room capture: the Mac microphone records bass, drum loop,
room sound, mistakes, energy, and the real session. This has the lowest
practice friction and avoids forcing users to understand audio routing or
interfaces before they can play.

The data model should still prepare for future capture levels:

```text
CaptureMode.room
CaptureMode.cleanBass
CaptureMode.multitrack
```

The journal should store recording references, loop metadata, marker
timestamps, derived clips, reflections, and producer notes separately enough
that clean bass input or multitrack capture can be added later without rewriting
the practice model.

Alternative considered: require clean bass capture from the beginning. That
would improve analysis and playback quality but would raise the onboarding
threshold too early.

### Treat Groove Journal As Personal Creative Memory

Every completed session creates a Groove Entry. The entry includes full take,
marked moments, selected clips, loop metadata, phase, challenge, reflection,
and producer note.

Groove Stream should optimize for listening back. It should show selected
moments and concise context first, with the full take available behind the
entry. A timeline of long recordings is not a useful creative archive.

Recordings are not grading evidence. They are memory, reflection, and raw
material for future personal musical identity.

### Use Built-In Loops Plus Imported Local Folders

The drum loop engine should begin with a small built-in pack across funk, soul,
hip-hop, neo soul, and shuffle. Each loop should carry metadata such as style,
tempo, feel, energy, and recommended phase. This keeps the first session from
starting with asset management.

Users should also be able to import local loop folders. Imported loops become
local objects and keep ordinary file references where practical. The app should
not require cloud import or a hidden proprietary loop library for V1.

### Make The Producer Agent Quiet

The producer agent has three V1 jobs:

- Plan today's session from the fixed spine and recent history.
- Reflect after the session with one short, grounded producer note.
- Curate titles, tags, and marked moments for Groove Stream.

The agent should not grade, lecture, or interrupt flow. Its voice should be
closer to a producer helping the user notice patterns:

```text
You stayed with the same idea longer today. Tomorrow, keep the rests but make
the octave answer softer.
```

V1 agent behavior can rely on metadata, reflection tags, markers, and session
history. Direct audio analysis can be added later.

### Keep Alan Integration Adapter-Based

The implementation boundary should stay layered:

```text
Groove Master domain core
  practice phases, plan generation, loop metadata, session lifecycle,
  journal schema

Audio runtime
  capture, playback, markers, export, future clean-input/multitrack support

Alan environment adapter
  objects, commands, buffers, views, queries, producer agent participation

macOS surface
  first UI and ContentInstance host
```

This keeps Groove Master portable while still making it a native environment
app. The first macOS implementation may run as an Alan creative app or
ContentInstance, but the domain core should not depend on the current Alan
macOS shell internals.

## Follow-Up Implementation Slices

The first implementation proposal should target one daily-use loop:

1. Groove Master domain models and local journal layout.
2. Built-in loop metadata plus local loop import.
3. Today Plan and Live Session macOS surface.
4. Room capture recording with marker timestamps.
5. End reflection and Groove Entry persistence.
6. Basic Groove Stream playback of marked moments and full takes.
7. Producer note generation from metadata and reflection only.

Later slices can add clean bass capture, multitrack capture, deeper audio
analysis, iOS/iPadOS capture, richer loop management, modal grammar, advanced
queries, export, and cross-device sync.
