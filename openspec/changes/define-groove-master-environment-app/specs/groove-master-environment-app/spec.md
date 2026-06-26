## ADDED Requirements

### Requirement: Groove Master Is A Real Creative Practice App
Groove Master SHALL be defined as a bass practice app for developing groove,
pocket, long-form playing, and personal musical feel. It SHALL live inside the
future Alan programmable environment as an environment app, but its primary
purpose SHALL be daily bass practice rather than proving Alan.

#### Scenario: Product is described
- **WHEN** product, architecture, or implementation docs describe Groove Master
- **THEN** they describe it as a creative bass practice app
- **AND** they do not reduce it to a demo, music-school lesson catalog, game,
  score tracker, or generic Alan shell feature

#### Scenario: Environment integration is scoped
- **WHEN** a future change integrates Groove Master with Alan
- **THEN** the change identifies how Groove Master maps to objects, commands,
  buffers, views, queries, and agent participation
- **AND** Groove Master domain logic remains separate from current macOS shell
  implementation details

### Requirement: V1 Provides A Complete Daily Practice Loop
V1 Groove Master SHALL support one complete daily loop from plan to recorded
journal entry.

#### Scenario: User starts daily practice
- **WHEN** the user opens Groove Master for daily practice
- **THEN** the app presents today's practice plan with phase, focus, session
  length, session blocks, selected metronome or loop source, inspiration
  reference when present, and one challenge

#### Scenario: User practices live
- **WHEN** the user starts the session
- **THEN** the app starts drum loop playback, timer, room-capture recording,
  and the active session view
- **AND** the session view provides a marker command for moments worth
  revisiting
- **AND** the session view does not display scores, ranks, combos, XP, or
  accuracy percentages

#### Scenario: User finishes the session
- **WHEN** the user ends the session
- **THEN** the app collects a short reflection
- **AND** it saves a Groove Entry with the recording take, markers, loop
  metadata, phase, challenge, reflection, and producer note when available

### Requirement: Practice Plan Uses Fixed Route And Adaptive Micro-Adjustment
Groove Master's practice plan SHALL use a concrete learning route with adaptive
daily details.

#### Scenario: Plan spine is inspected
- **WHEN** the user or agent inspects the practice model
- **THEN** the model includes phases for Feel Time, Pocket, Groove Construction,
  Bass Language, and Musical Conversation
- **AND** theory-heavy concepts appear after groove and pocket foundations

#### Scenario: Twelve-month route is inspected
- **WHEN** the user or agent inspects the long-term journey
- **THEN** Weeks 1-4 focus on one note, pulse, silence, breathing, and stable
  sound
- **AND** Weeks 5-8 focus on pocket with E, B, high E, root/rest, and
  root/octave patterns
- **AND** Weeks 9-12 focus on loop construction, repetition, variation, and
  rhythmic imitation without forced song learning
- **AND** Months 3-6 introduce bass language such as root, fifth, octave,
  ghost note, and passing tone
- **AND** Months 6-12 focus on musical conversation, original basslines, and
  deeper listening

#### Scenario: Daily plan adapts
- **WHEN** the app prepares today's practice
- **THEN** it may adjust loop selection, session length, challenge, review
  target, continuation from a prior groove, or phase pacing
- **AND** it bases V1 adaptation on durable signals such as completed sessions,
  uninterrupted play time, markers, reflection tags, loop style, skipped days,
  and producer notes
- **AND** it does not require automatic timing or pitch analysis for V1

### Requirement: Session Plans Are Built From Practice Blocks
Groove Master SHALL model each daily session as a small sequence of practice
blocks rather than as one undifferentiated timer.

#### Scenario: Practice block is represented
- **WHEN** a day plan is generated
- **THEN** each practice block can specify duration, source, allowed notes,
  focus, challenge, recording policy, and success reflection
- **AND** supported V1 sources include metronome, drum loop, inspiration
  reference, and silence

#### Scenario: Early feel-time plan is generated
- **WHEN** the user is in early Feel Time
- **THEN** the app can generate metronome blocks such as 80 BPM open E,
  silence blocks such as E plus rest, and simple reference-following blocks
- **AND** the plan does not introduce theory, scales, chord names, or song
  transcription

#### Scenario: Weekly cadence is generated
- **WHEN** the app schedules a week
- **THEN** it can use archetypes such as Metronome, Drum Loop, Steal Groove,
  Free Jam, Recording, Deep Listening, and Listening Only
- **AND** Listening Only days are allowed as valid practice days

### Requirement: Audio Model Starts With Room Capture And Stays Progressive
Groove Master SHALL default V1 recording to room capture while preserving an
audio model that can later support clean bass capture and multitrack sessions.

#### Scenario: V1 session records audio
- **WHEN** a V1 session is recorded
- **THEN** the default capture mode records the room using the Mac microphone
- **AND** the app does not require an external audio interface, clean routing,
  or multitrack setup before first use

#### Scenario: Recording data is persisted
- **WHEN** a session recording is saved
- **THEN** the journal stores recording take references, marker timestamps,
  derived clip references when available, loop metadata, capture mode,
  reflection, and producer note separately enough to support future capture
  modes

### Requirement: Groove Journal Prioritizes Listenability
Groove Master SHALL treat recordings as personal creative memory rather than
grading evidence.

#### Scenario: Session becomes a journal entry
- **WHEN** a session completes
- **THEN** the full recording remains available
- **AND** marked moments or selected clips are surfaced first in Groove Stream
- **AND** the full take does not become the only timeline representation

#### Scenario: Journal is reviewed
- **WHEN** the user reviews Groove Stream
- **THEN** entries show concise musical context such as date, phase, loop,
  challenge, reflection, producer note, and selected moments
- **AND** the app does not present recording analysis as a grade

### Requirement: Drum Loops Use Built-In Pack And Local Import
Groove Master SHALL provide a small built-in drum loop pack and support imported
local loop folders.

#### Scenario: First session starts without imports
- **WHEN** the user has not imported any loops
- **THEN** the app can still choose a built-in loop for today's practice

#### Scenario: User imports local loops
- **WHEN** the user imports a local loop folder
- **THEN** the app indexes loops as local objects with metadata such as style,
  tempo, feel, energy, and recommended phase when available
- **AND** it does not require cloud upload or a hidden proprietary loop library
  for V1

### Requirement: Inspiration Cards Reference Songs Without Forced Repertoire
Groove Master SHALL use Inspiration Cards to provide musical taste and direction
without requiring the user to learn specific songs.

#### Scenario: Inspiration card is shown
- **WHEN** today's plan includes an inspiration card
- **THEN** the card may name an artist, song, groove, or reference
- **AND** it pairs that reference with a conceptual challenge such as using
  fewer notes, leaving more space, staying relaxed, or repeating longer

#### Scenario: User follows inspiration
- **WHEN** the user starts a session with an inspiration reference
- **THEN** the task remains about feel, rhythm, space, or groove concept
- **AND** the app does not present the reference as required repertoire,
  tablature, score, or song tutorial
- **AND** early practice may use a reference track or playlist item for
  one-note feel-following without turning it into bundled song curriculum

### Requirement: Pocket Tracker Provides Non-Graded Reflection
Groove Master SHALL track groove habits as reflection rather than evaluation.

#### Scenario: Pocket tracker is updated
- **WHEN** a session completes
- **THEN** the app may record continuous play time, space usage, consistency,
  flow breaks, and selected-moment duration
- **AND** these values are stored as reflective practice signals rather than
  grades

#### Scenario: Pocket tracker is displayed
- **WHEN** the user reviews progress
- **THEN** the app may show reflective copy such as "17m without stopping",
  "You left more room today", or "Your timing is becoming more stable"
- **AND** it does not display accuracy percentages, rank, combo, pass/fail
  language, or school-like scores

### Requirement: Product Presentation Avoids Education Gamification
Groove Master SHALL use an instrument-first creative environment presentation
rather than a bright education, music-school, or gamified trainer presentation.

#### Scenario: Groove Master surface is designed
- **WHEN** a user-facing Groove Master surface is designed
- **THEN** it follows the visual direction of dark, indigo, midnight purple,
  acid green accent, industrial minimalism, and Future UNIX
- **AND** it avoids cartoon gamification, bright education UI, dense dashboards,
  and traditional music-school aesthetics

#### Scenario: Groove Master is hosted in Alan
- **WHEN** Groove Master appears inside an Alan environment surface
- **THEN** its distinctive visual identity remains scoped to the Groove Master
  app surface
- **AND** it does not require the current Alan shell or other environment apps
  to adopt Groove Master's palette or interaction style

### Requirement: Commercial Features Are Deferred From V1
Groove Master's V1 SHALL preserve a future business posture without
implementing commerce.

#### Scenario: V1 scope is proposed
- **WHEN** a V1 implementation scope is proposed
- **THEN** it may reserve product space for future Pro capabilities such as
  groove packs, advanced drum grooves, recording analysis, and community
  challenges
- **AND** it does not require payment, subscriptions, marketplace, or community
  features for the first daily-use loop

### Requirement: Producer Agent Is Low Presence
The Groove Master producer agent SHALL plan, reflect, and curate without
grading, lecturing, or interrupting musical flow.

#### Scenario: Producer prepares practice
- **WHEN** the app prepares today's plan
- **THEN** the producer agent may choose or adjust the session using the fixed
  spine and recent history

#### Scenario: Producer reflects after practice
- **WHEN** a session completes
- **THEN** the producer agent may write one short note grounded in session
  metadata, markers, and user reflection
- **AND** the note avoids scores, ranking language, and corrective school-like
  feedback

#### Scenario: Producer curates journal
- **WHEN** the journal entry is created
- **THEN** the producer agent may suggest a title, tags, or moments to feature
- **AND** raw recordings remain inspectable and are not hidden by curation

### Requirement: Alan Integration Is Adapter-Based
Groove Master SHALL separate domain core, audio runtime, Alan environment
adapter, and macOS surface boundaries.

#### Scenario: Implementation architecture is proposed
- **WHEN** a future implementation change is proposed
- **THEN** it identifies the domain core boundary for practice phases, plan
  generation, practice blocks, inspiration cards, pocket tracking, loop
  metadata, session lifecycle, and journal schema
- **AND** it identifies the audio runtime boundary for capture, playback,
  markers, export, and future capture modes
- **AND** it identifies the Alan adapter boundary for objects, commands,
  buffers, views, queries, and agent participation
- **AND** it identifies the first macOS surface without making the domain core
  depend on current Alan shell internals

#### Scenario: Groove Master content opens in Alan
- **WHEN** a future Alan surface opens Groove Master content
- **THEN** a Groove Session, Groove Entry, Loop Library, or Groove Stream can be
  represented as environment content
- **AND** commands remain addressable by UI, command palette, modal grammar,
  automation, or agent where those invocation layers exist
