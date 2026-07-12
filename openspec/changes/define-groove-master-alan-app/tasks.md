## 1. Domain Core And App Tree

- [ ] 1.1 Implement portable practice-route, plan, block, session, marker,
  reflection, Pocket Tracker, loop, recording, and Groove Entry domain models.
- [ ] 1.2 Implement the Groove Master aP adapter and in-memory
  `/mnt/groove-master` tree with snapshot, document, events, and `ctl` semantics.
- [ ] 1.3 Add domain/aP tests proving the domain core does not depend on Alan
  Kernel, AgentFS, SwiftUI, or host framework types.

## 2. Durability And Service Manager

- [ ] 2.1 Add app-owned durable backing for plans, sessions, journal, loops,
  recordings, and producer proposals.
- [ ] 2.2 Post `/srv/groove-master`, mount `/mnt/groove-master`, and enforce
  filtered-handle/access-right behavior.
- [ ] 2.3 Add restart, retention, and recording-reference tests.

## 3. Alan For macOS Client

- [ ] 3.1 Confirm Alan for macOS can open, watch, and write the mounted Groove
  Master tree directly; keep this section blocked until that entry criterion is
  met.
- [ ] 3.2 Render Today Plan, journal, marked moments, and loops from app-tree
  fixtures through the direct file client.
- [ ] 3.3 Add Alan Dev visual, keyboard, accessibility, and terminal-first host
  verification.

## 4. Live Session And Audio

- [ ] 4.1 Add metronome, loop playback, and room capture behind the audio backend.
- [ ] 4.2 Wire start/mark/end/cancel and reflection commit through canonical app
  files and owning `ctl` surfaces.
- [ ] 4.3 Add fake-audio tests and one manual end-to-end room-capture session.

## 5. Producer Agent Process

- [ ] 5.1 Add producer Agent Executable/Skill packaging and bounded descriptor
  assembly.
- [ ] 5.2 Spawn the producer with no raw-audio or committed-journal write access;
  persist suggestions only in app-owned proposal files.
- [ ] 5.3 Add proposal review/commit tests and `/proc`/`/agent` provenance checks.

## 6. Verification And Archive Readiness

- [ ] 6.1 Confirm the host surface has no alternate Groove Master authority and
  consumes only the mounted aP tree.
- [ ] 6.2 Run focused Rust/Swift tests, Alan Dev UI verification, and relevant
  workspace checks.
- [ ] 6.3 Run strict validation for this change and the full OpenSpec tree.
- [ ] 6.4 After merge, sync `groove-master-alan-app` into canonical specs before
  archiving.
