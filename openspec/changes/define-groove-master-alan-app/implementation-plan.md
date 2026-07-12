# Groove Master Alan App Implementation Plan

The app-owned file tree lands before rich host UI so every later surface uses
the same authority.

## Phase 1: Domain Core And In-Memory Tree

1. Implement portable practice-route, day-plan, block, session, marker,
   reflection, Pocket Tracker, and Groove Entry models.
2. Implement the Groove Master aP adapter with deterministic in-memory fixtures
   for `/mnt/groove-master`.
3. Pin document commit, session `ctl`, collection events, and process-safe audio
   metadata contracts with focused tests.

Exit evidence: domain tests, aP walk/read/write/clunk tests, snapshot/event
fixtures, no dependency from domain core to Alan Kernel or SwiftUI.

## Phase 2: Durable Journal And Service Lifecycle

1. Add app-owned backing storage for plans, sessions, journal entries, loops,
   recordings, and producer proposals.
2. Post `/srv/groove-master`; have Service Manager mount
   `/mnt/groove-master`.
3. Verify restart recovery and retention of referenced recordings.

Exit evidence: restart test, handle visibility/access test, mounted-tree smoke.

## Phase 3: Alan For macOS Read-Only Client

Entry criterion: Alan for macOS can open, watch, and read the mounted Groove
Master tree directly. Until that exists, this phase remains parked.

1. Render Today Plan, recent Groove Entries, marked moments, and loop metadata
   by reading the mounted tree.
2. Add visual and accessibility verification without moving domain state into
   SwiftUI models.

Exit evidence: Alan Dev screenshot/smoke, file-fixture renderer test, and direct
aP read/watch evidence.

## Phase 4: Live Session And Audio

1. Add metronome/loop playback and room capture behind the audio backend.
2. Wire start/mark/end/cancel through session/today `ctl` and reflection document
   commit.
3. Publish bounded elapsed/status/marker events off the real-time audio path.

Exit evidence: fake-audio lifecycle tests, cancellation safety, manual room-
capture session on Alan Dev.

## Phase 5: Producer Agent Process

1. Package the producer Agent Executable and Skill.
2. Assemble a bounded namespace from plan, recent summaries, loop/marker
   metadata, reflection, and writable proposal files.
3. Spawn the producer, show proposals, and require app/user commit.

Exit evidence: namespace assertion, `/proc`/`/agent` visibility, no raw-audio or
committed-journal write access, proposal accept/discard test.

## Phase 6: Verification And Closure

1. Confirm Alan for macOS uses only direct aP consumption for Groove Master.
2. Run focused Rust/Swift verification, full OpenSpec validation, spec sync, and
   archive readiness review.
