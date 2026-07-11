## Context

Groove Master helps a bassist build pocket, long-form playing, and personal feel
through a daily practice loop. The product domain is richer than the shell that
hosts it: practice phases, day plans, audio sources, sessions, recordings,
markers, reflections, journal entries, and producer suggestions must remain
portable and app-owned.

The prior design mapped those concepts onto a generic programmable environment
and planned direct `ShellContentInstance` wiring. ADR-0024 retired the generic
Object/Buffer/View/Command/Query ontology, and
the `alan-app-service-integration` capability folded into
`remove-daemon-era-contracts` establishes the durable boundary:
domain core → aP adapter → `/srv` handle → `/mnt` tree → symmetric UI/Tool/agent
file clients.

## Goals / Non-Goals

**Goals:**

- Define a complete, calm daily bass-practice loop worth using independently of
  Alan's architecture demonstration value.
- Keep practice and journal semantics in a portable Groove Master domain core.
- Expose the app as a self-describing file tree with durable app-owned state.
- Keep room capture and playback behind a progressive audio backend.
- Use a low-presence producer Agent Process for planning, reflection, and
  curation without grading or interruption.
- Keep the first Alan for macOS experience native and focused.

**Non-Goals:**

- Music-school lessons, scoring, ranks, XP, accuracy grading, or real-time
  corrective coaching.
- Clean-input or multitrack capture in V1.
- Automatic pitch/timing analysis in V1.
- Commerce, marketplace, community challenges, or cloud sync in V1.
- A generic Alan app object framework or new Kernel primitives.
- Making `ShellContentInstance` or SwiftUI state the domain source of truth.

## Decisions

### 1. Groove Master owns four explicit layers

```text
Groove Master domain core
  practice route, plans, blocks, session lifecycle, journal, pocket signals

Audio backend
  metronome, loop playback, room capture, markers, export, later multitrack

Groove Master aP adapter
  app tree, commit semantics, events, ctl, persistence projection

Alan for macOS client
  Today Plan, Live Session, reflection, journal and playback presentation
```

The domain core is file-unaware where practical. The adapter translates whole
document writes and lifecycle controls into domain commands. The audio backend
may use Apple frameworks but does not leak framework types into the app tree.

### 2. The app tree is the integration contract

The service posts `/srv/groove-master` and mounts at `/mnt/groove-master`:

```text
/mnt/groove-master/
├── status
├── today/
│   ├── plan
│   ├── events
│   └── ctl
├── sessions/
│   ├── events
│   └── <session-id>/
│       ├── plan
│       ├── status
│       ├── elapsed
│       ├── markers
│       ├── reflection
│       ├── recording
│       ├── events
│       └── ctl
├── journal/
│   ├── events
│   └── <entry-id>/...
├── loops/
│   ├── built-in/...
│   └── imported/...
├── inspiration/...
├── pocket/summary
└── producer/
    ├── proposals/
    └── events
```

Plans and reflections are whole documents committed on clunk. Session start,
stop, cancel, and marker control belong to the session/today `ctl` owner.
Dynamic collections expose offset-resumable events. Recording content may be a
descriptor/path into app-owned storage rather than inline bytes.

### 3. The daily loop remains plan → play → mark → reflect → revisit

Today Plan shows phase, focus, duration, practice blocks, source, inspiration,
and one challenge. Live Session starts the selected metronome/loop, timer, room
capture, and marker control without score UI. End Reflection is short. Commit
creates a Groove Entry with the take, marked moments, source metadata, phase,
challenge, reflection, Pocket Tracker snapshot, and optional producer note.

Groove Stream surfaces marked moments and clips before full takes while keeping
raw recordings available.

### 4. Practice uses a fixed spine with adaptive micro-adjustment

The long route remains Feel Time → Pocket → Groove Construction → Bass Language
→ Musical Conversation. V1 adapts loop, duration, challenge, review target,
continuation, and pacing from completed sessions, uninterrupted play time,
markers, reflection tags, skipped days, loop style, and producer notes. It does
not require automatic pitch or timing analysis.

### 5. Audio starts with room capture and preserves future lanes

V1 uses the Mac microphone, metronome, built-in/imported drum loops, inspiration
references, and silence. Recording metadata separates capture mode, take,
markers, derived clips, source, reflection, and producer note so clean input and
multitrack can arrive later without replacing the journal schema.

### 6. Producer work is bounded Agent Executable spawn

The app opens the current route, recent journal summaries, relevant loop and
marker metadata, user reflection, producer Skill, and a writable proposal
directory. It spawns the configured producer Agent Executable with only those
descriptors. The process may propose today's plan adjustments, one short
post-session note, titles, tags, or featured moments.

The producer does not receive raw audio by default, cannot mutate committed
journal entries directly, and cannot grade. Proposals remain inspectable files;
the app or user commits accepted changes.

### 7. Alan for macOS is a file client with a temporary bridge if required

The native UI reads snapshots and events, writes reflection/plan documents, and
writes owning `ctl` commands. During the parked macOS migration it may use a
named `GrooveMasterHostCompatibilityBridge` that translates current shell
content actions into those file operations. The bridge owns no domain state,
cannot add bridge-only behavior, and is deleted when the surface reads the aP
tree directly.

### 8. Product presentation stays instrument-first

Groove Master's app surface may use its dark indigo/midnight palette, restrained
acid-green signal color, and industrial/Future-UNIX feel while the Alan shell
remains calm and material-driven. The app avoids dashboard density, cartoons,
gamification, and school-like correction.

## Risks / Trade-offs

- [Risk] The file tree becomes a method-per-file RPC façade → Mitigation: keep
  durable resources readable, documents whole, lifecycle in adjacent `ctl`, and
  observation in streams.
- [Risk] Audio callbacks and aP operations contend → Mitigation: audio backend
  owns real-time work; adapter publishes bounded snapshots/events off the audio
  thread.
- [Risk] Producer suggestions feel intrusive → Mitigation: low presence, one
  bounded note, proposal files, and no real-time interruption.
- [Risk] Compatibility bridge becomes permanent → Mitigation: no bridge-only
  behavior and an explicit file-native deletion gate.
- [Risk] First slice is still too broad → Mitigation: phase by domain tree,
  read-only renderer, then live audio, then producer spawn.

## Migration Plan

1. Implement domain models and in-memory aP tree with deterministic fixtures.
2. Add durable journal/loop backing and service registration/mounting.
3. Add Alan for macOS read-only Today/Journal client through the temporary host
   bridge if necessary.
4. Add Live Session audio, marker, reflection, and commit control.
5. Add producer Agent Executable spawn and proposal review.
6. Delete the host bridge when the macOS surface consumes the aP tree directly.

## Open Questions

- Whether the first recording file is exposed directly from app storage or
  through a streaming file served by the adapter.
- Whether producer plan proposals auto-apply when purely non-destructive or
  always wait for explicit app commit in V1.
