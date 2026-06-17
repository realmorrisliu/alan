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
  length, selected drum loop, and one challenge

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

### Requirement: Practice Plan Uses Fixed Spine And Adaptive Micro-Adjustment
Groove Master's practice plan SHALL use a fixed learning spine with adaptive
daily details.

#### Scenario: Plan spine is inspected
- **WHEN** the user or agent inspects the practice model
- **THEN** the model includes phases for Groove, Pocket, Jam, Language, and
  Vocabulary
- **AND** theory-heavy concepts appear after groove and pocket foundations

#### Scenario: Daily plan adapts
- **WHEN** the app prepares today's practice
- **THEN** it may adjust loop selection, session length, challenge, review
  target, continuation from a prior groove, or phase pacing
- **AND** it bases V1 adaptation on durable signals such as completed sessions,
  uninterrupted play time, markers, reflection tags, loop style, skipped days,
  and producer notes
- **AND** it does not require automatic timing or pitch analysis for V1

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
  generation, loop metadata, session lifecycle, and journal schema
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
