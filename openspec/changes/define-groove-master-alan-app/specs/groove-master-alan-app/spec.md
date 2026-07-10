## ADDED Requirements

### Requirement: Groove Master is a real Alan App
Groove Master SHALL be a bass-practice product for developing groove, pocket,
long-form playing, and personal musical feel. It SHALL keep app-domain authority
outside Alan Kernel and SHALL integrate through an app-owned aP file tree rather
than a generic environment object framework.

#### Scenario: Product is described
- **WHEN** product or architecture docs describe Groove Master
- **THEN** they describe a real creative-practice Alan App
- **AND** they do not reduce it to a demo, generic shell feature, school lesson
  catalog, game, or Object/Buffer/View/Query framework

### Requirement: V1 provides a complete daily practice loop
Groove Master SHALL support Today Plan, Live Session, markers, End Reflection,
and Groove Entry creation as one complete V1 loop.

#### Scenario: User practices and finishes
- **WHEN** the user starts and completes today's session
- **THEN** the app runs the selected source, timer, room capture, and marker
  control, then collects a short reflection
- **AND** it commits a Groove Entry containing the take, markers, source metadata,
  phase, challenge, reflection, Pocket Tracker snapshot, and optional producer
  note
- **AND** it presents no score, rank, combo, XP, or accuracy percentage

### Requirement: Practice uses a fixed route with adaptive micro-adjustment
Groove Master SHALL use the ordered route Feel Time, Pocket, Groove Construction,
Bass Language, and Musical Conversation while adapting bounded daily details
from durable practice signals.

#### Scenario: Early route is generated
- **WHEN** the user is in the first twelve weeks
- **THEN** plans progress from one-note pulse/space, through pocket and
  root/rest/root-octave patterns, into loop construction and variation
- **AND** theory-heavy language follows groove and pocket foundations

#### Scenario: Daily plan adapts
- **WHEN** the app prepares a later session
- **THEN** it may adjust source, duration, challenge, review target,
  continuation, and pacing from completed sessions, play time, markers,
  reflections, skipped days, and producer notes
- **AND** V1 does not require automatic pitch or timing analysis

### Requirement: Session plans are sequences of practice blocks
Each day plan SHALL contain bounded practice blocks with duration, source,
allowed notes, focus, challenge, recording policy, and reflection prompt.

#### Scenario: Weekly cadence is generated
- **WHEN** Groove Master schedules a practice week
- **THEN** it may use Metronome, Drum Loop, Steal Groove, Free Jam, Recording,
  Deep Listening, and Listening Only archetypes
- **AND** Listening Only is a valid practice day

### Requirement: Audio begins with room capture and remains progressive
V1 SHALL use room capture through the Mac microphone while preserving separate
capture-mode, take, marker, clip, source, reflection, and producer-note fields
for future clean-input and multitrack support.

#### Scenario: First recording is created
- **WHEN** the user records a V1 session
- **THEN** no external audio interface or multitrack setup is required
- **AND** the resulting app files retain enough separated metadata for later
  capture modes

### Requirement: Groove Journal prioritizes listenability
Groove Master SHALL treat recordings as personal creative memory rather than
grading evidence and SHALL surface marked moments before full takes while keeping
the raw take available.

#### Scenario: User reviews Groove Stream
- **WHEN** a journal entry has marked moments or derived clips
- **THEN** those moments appear before the full take with concise musical context
- **AND** the recording is not presented as a grade

### Requirement: Drum loops support built-in and local sources
Groove Master SHALL ship a small built-in loop pack and index authorized local
loop folders without requiring cloud upload or a hidden proprietary library.

#### Scenario: User imports a loop folder
- **WHEN** an authorized local folder is mounted or selected
- **THEN** the app indexes loop files with available style, tempo, feel, energy,
  and recommended-phase metadata
- **AND** the source remains a local file tree

### Requirement: Inspiration guides without forced repertoire
Inspiration Cards SHALL reference artists, songs, grooves, or listening examples
as prompts for feel, rhythm, space, restraint, or repetition rather than required
song lessons.

#### Scenario: Inspiration is used in practice
- **WHEN** a plan includes an inspiration reference
- **THEN** it pairs the reference with a conceptual challenge
- **AND** it does not require tablature, scoring, or exact repertoire completion

### Requirement: Pocket Tracker is reflective rather than evaluative
Pocket Tracker SHALL record bounded signals such as uninterrupted play time,
space usage, consistency, flow breaks, and selected-moment duration using
reflective language rather than grades.

#### Scenario: Progress is shown
- **WHEN** the user reviews recent practice
- **THEN** copy may describe flow, space, or continuity
- **AND** it omits rank, pass/fail, combo, and school-like correction

### Requirement: Producer Agent Process has low presence
The producer SHALL plan, reflect, and curate through an explicitly spawned Agent
Process with bounded descriptors. It SHALL NOT grade, lecture, interrupt live
playing, receive raw audio by default, or directly mutate committed journal
state.

#### Scenario: Producer reflects after practice
- **WHEN** the app opens recent plan, marker, recording metadata, and reflection
  descriptors and spawns the producer Agent Executable
- **THEN** the producer may write one short grounded note or curation proposal
  into the authorized proposal directory
- **AND** the app or user controls final commit

### Requirement: Groove Master owns a mounted service tree
The Groove Master adapter SHALL post `/srv/groove-master` and Service Manager
SHALL mount its app tree at `/mnt/groove-master`. The tree SHALL expose Today,
sessions, journal, loops, inspiration, Pocket Tracker, producer proposals,
snapshot files, dynamic events streams, and adjacent lifecycle `ctl` files.

#### Scenario: Alan OS starts Groove Master
- **WHEN** the Groove Master service becomes ready
- **THEN** its access-filtered handle is posted and its tree is mounted
- **AND** Alan Kernel gains no Groove Master-specific type or persistence logic

#### Scenario: Session state changes
- **WHEN** a client starts, marks, ends, or cancels a session
- **THEN** it writes the owning session/today `ctl` or a whole document committed
  on clunk
- **AND** watchers learn the result from snapshot files and events

### Requirement: Alan for macOS is a Groove Master file client
Alan for macOS SHALL render and operate Groove Master from the mounted app tree.
Any temporary shell-content bridge SHALL translate to canonical file operations,
own no domain truth, add no bridge-only behavior, and document deletion when the
surface reads aP directly.

#### Scenario: The first macOS surface opens
- **WHEN** the parked macOS host still needs a compatibility bridge
- **THEN** the implementation names the bridge and the file operations it
  translates
- **AND** the authoritative plan, session, journal, and producer state remains in
  `/mnt/groove-master`

### Requirement: Product presentation is instrument-first
The Groove Master app surface SHALL use a focused creative-instrument aesthetic
and MAY use dark indigo, midnight purple, restrained acid-green signals, and
industrial/Future-UNIX cues while keeping that identity scoped to the app.

#### Scenario: App UI is reviewed
- **WHEN** Groove Master is rendered inside Alan for macOS
- **THEN** it avoids bright education UI, cartoon gamification, dense dashboards,
  and traditional school aesthetics
- **AND** it does not force the Alan shell or other Alan Apps to adopt its palette

### Requirement: Commercial features are deferred from V1
Groove Master V1 SHALL NOT require payment, subscriptions, marketplace,
community challenges, or paid loop packs.

#### Scenario: First implementation is scoped
- **WHEN** V1 tasks are selected
- **THEN** they prioritize the complete daily loop and local journal
- **AND** commercial and community work remains a separate future change
