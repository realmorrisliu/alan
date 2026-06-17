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
  selected drum loop, and challenge.
- [ ] 2.2 Define Live Session behavior with drum loop playback, timer,
  room-capture recording, marker command, optional micro-prompts, and no
  scoring UI.
- [ ] 2.3 Define End Reflection and Groove Entry creation.
- [ ] 2.4 Define Groove Stream as selected moments first, full takes available
  behind entries.

## 3. Practice, Audio, And Loop Models

- [ ] 3.1 Define the fixed learning spine: Groove, Pocket, Jam, Language, and
  Vocabulary.
- [ ] 3.2 Define adaptive micro-adjustment inputs for V1 without requiring
  automatic timing or pitch analysis.
- [ ] 3.3 Define progressive capture modes with V1 room capture and future
  clean bass or multitrack support.
- [ ] 3.4 Define built-in drum loop pack plus imported local loop folder model.
- [ ] 3.5 Define recording, marker, clip, loop reference, reflection, and
  producer note persistence expectations.

## 4. Producer Agent

- [ ] 4.1 Define producer planning behavior from fixed spine and recent history.
- [ ] 4.2 Define producer reflection after a session using metadata, markers,
  and user reflection.
- [ ] 4.3 Define producer curation for titles, tags, and selected moments.
- [ ] 4.4 Explicitly exclude grading, ranks, accuracy percentages, real-time
  correction, and school-like lecturing.

## 5. Follow-Up Slice Decomposition

- [ ] 5.1 Identify the first implementation slice as one daily-use loop:
  plan, loop, room recording, markers, reflection, journal, and basic producer
  note.
- [ ] 5.2 Defer clean bass capture, multitrack capture, deep audio analysis,
  iOS/iPadOS capture, advanced loop management, modal grammar, export, and
  cross-device sync to later focused changes.
- [ ] 5.3 Keep the first macOS/Alan surface adapter-based so the Groove Master
  domain core remains portable.

## 6. Verification

- [ ] 6.1 Run `openspec validate define-groove-master-environment-app --strict`.
- [ ] 6.2 Run `git diff --check -- openspec/changes/define-groove-master-environment-app`.
- [ ] 6.3 Review proposal, design, spec, and tasks for placeholders,
  contradictions, overbroad V1 scope, and unclear Alan/Groove Master
  boundaries.
- [ ] 6.4 Confirm the change does not modify current Alan runtime, daemon,
  terminal, macOS shell, or programmable-environment constitution behavior.
