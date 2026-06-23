## 0. Programmable Environment Alignment

- [x] 0.1 Record Groove Master as an environment app with a portable domain core, separate audio runtime, Alan environment adapter, and host-surface boundary; keep current Alan shell internals out of the domain model

## 1. Product Boundary

- [ ] 1.1 Define Groove Master as a real bass practice app inside the future
  Alan programmable environment rather than as an Alan proof-of-concept.
- [ ] 1.2 Define the app's environment objects, commands, buffers, views,
  queries, and producer-agent role.
- [ ] 1.3 Preserve Groove Master ownership of music-practice logic while Alan
  owns environment conventions, local object/runtime contracts, command
  mediation, and future ContentInstance hosting.

## 2. V1 Daily Practice Loop

- [ ] 2.1 Define the Today Plan surface with phase, focus, session length,
  practice blocks, selected metronome or drum loop source, inspiration
  reference, and challenge.
- [ ] 2.2 Define Live Session behavior with metronome/drum-loop playback,
  timer, room-capture recording, marker command, optional micro-prompts, and
  no scoring UI.
- [ ] 2.3 Define End Reflection and Groove Entry creation.
- [ ] 2.4 Define Groove Stream as selected moments first, full takes available
  behind entries.

## 3. Practice, Audio, And Loop Models

- [ ] 3.1 Define the fixed learning route: Feel Time, Pocket, Groove
  Construction, Bass Language, and Musical Conversation.
- [ ] 3.2 Define adaptive micro-adjustment inputs for V1 without requiring
  automatic timing or pitch analysis.
- [ ] 3.3 Define progressive capture modes with V1 room capture and future
  clean bass or multitrack support.
- [ ] 3.4 Define built-in drum loop pack plus imported local loop folder model.
- [ ] 3.5 Define metronome, drum loop, inspiration reference, and silence as V1
  practice block sources.
- [ ] 3.6 Define weekly cadence archetypes: Metronome, Drum Loop, Steal Groove,
  Free Jam, Recording, Deep Listening, and Listening Only.
- [ ] 3.7 Define recording, marker, clip, loop reference, reflection, pocket
  tracker, and producer note persistence expectations.

## 4. Inspiration And Pocket Tracking

- [ ] 4.1 Define Inspiration Cards as references for taste and exploration, not
  forced song-learning tasks.
- [ ] 4.2 Define Pocket Tracker around continuous play time, space usage,
  consistency, flow breaks, and selected-moment duration.
- [ ] 4.3 Explicitly keep Pocket Tracker copy reflective rather than evaluative.
- [ ] 4.4 Define product presentation as instrument-first, dark, indigo,
  midnight-purple, acid-green, industrial-minimal, and Future UNIX rather than
  bright education or gamified trainer UI.
- [ ] 4.5 Record future Free/Pro posture while keeping commerce, subscriptions,
  marketplace, and community features out of V1.

## 5. Producer Agent

- [ ] 5.1 Define producer planning behavior from fixed route and recent history.
- [ ] 5.2 Define producer reflection after a session using metadata, markers,
  and user reflection.
- [ ] 5.3 Define producer curation for titles, tags, and selected moments.
- [ ] 5.4 Explicitly exclude grading, ranks, accuracy percentages, real-time
  correction, and school-like lecturing.

## 6. Follow-Up Slice Decomposition

- [ ] 6.1 Identify the first implementation slice as one daily-use loop:
  plan, metronome/loop source, room recording, markers, reflection, journal,
  pocket tracker snapshot, and basic producer note.
- [ ] 6.2 Defer clean bass capture, multitrack capture, deep audio analysis,
  iOS/iPadOS capture, advanced loop management, modal grammar, export, and
  cross-device sync to later focused changes.
- [ ] 6.3 Keep the first macOS/Alan surface adapter-based so the Groove Master
  domain core remains portable.

## 7. Verification

- [ ] 7.1 Run `openspec validate define-groove-master-environment-app --strict`.
- [ ] 7.2 Run `git diff --check -- openspec/changes/define-groove-master-environment-app`.
- [ ] 7.3 Review proposal, design, spec, and tasks for placeholders,
  contradictions, overbroad V1 scope, and unclear Alan/Groove Master
  boundaries.
- [ ] 7.4 Confirm the change does not modify current Alan runtime, daemon,
  terminal, macOS shell, or programmable-environment constitution behavior.
