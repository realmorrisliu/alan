# Groove Master V1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first Alan for macOS Groove Master environment app slice: Today Plan, practice blocks, Groove Journal persistence, Pocket Tracker signals, and a minimal hosted Groove Master content surface.

**Architecture:** Add a portable Groove Master domain core under the macOS client first, then expose it through the existing shell content-container system as a new non-terminal content kind. Keep recording, journal, and Alan hosting behind small interfaces so the domain model can move to a shared Apple module later.

**Tech Stack:** Swift 6, SwiftUI, Foundation JSON encoding, AVFoundation behind an adapter, existing Alan shell content containers, existing script-driven Swift contract tests.

---

## Scope

This plan implements the first daily-use loop skeleton. It does not implement payments, community, mobile capture, clean bass capture, multitrack capture, deep audio analysis, or a full DAW.

The first slice must produce working, testable software:

```text
Open Groove Master tab
  -> see today's plan
  -> start/end an in-memory session
  -> mark moments
  -> persist a Groove Entry JSON file
  -> see the entry in a local Groove Stream list
```

Room capture is introduced behind `GrooveAudioCaptureClient`, with a fake client used in tests and an AVFoundation client wired but not required for script tests.

## File Structure

- Create `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterModels.swift`
  - Pure domain types: phases, sources, blocks, plans, recordings, markers, reflections, tracker snapshots, journal entries.
- Create `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift`
  - Deterministic plan generation from date, journey start date, and history.
- Create `clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift`
  - Local JSON persistence for entries and imported loop metadata.
- Create `clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift`
  - Protocol, fake recorder, and AVFoundation-backed room-capture client.
- Create `clients/apple/alan-macos/Views/GrooveMaster/GrooveMasterContentView.swift`
  - First SwiftUI content surface hosted by `TerminalPaneView`.
- Modify `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift`
  - Add Groove Master content kind, intent, payload, and capability.
- Modify `clients/apple/alan-macos/Services/Shell/ShellContentRenderingRegistry.swift`
  - Route Groove Master content to a new render kind and icon.
- Modify `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
  - Mount Groove Master content descriptors from shell intents.
- Modify `clients/apple/alan-macos/ShellHostController.swift`
  - Add `openGrooveMasterTab`.
- Modify `clients/apple/alan-macos/TerminalPaneView.swift`
  - Dispatch the bounded content leaf to `GrooveMasterContentView`.
- Modify `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift`
  - Provide the sidebar type label `Groove`.
- Create `clients/apple/scripts/test-groove-master-domain.swift`
  - Domain, plan engine, journal store, and fake capture tests.
- Create `clients/apple/scripts/test-groove-master-domain.sh`
  - Compiles and runs the focused domain tests.
- Modify `clients/apple/scripts/test-shell-runtime-metadata.swift`
  - Adds content-container contract checks for Groove Master content.

---

### Task 1: Groove Master Domain Models

**Files:**
- Create: `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterModels.swift`
- Create: `clients/apple/scripts/test-groove-master-domain.swift`
- Create: `clients/apple/scripts/test-groove-master-domain.sh`

- [ ] **Step 1: Write failing domain tests**

Create `clients/apple/scripts/test-groove-master-domain.swift`:

```swift
import Foundation

private enum TestFailure: Error, CustomStringConvertible {
    case message(String)

    var description: String {
        switch self {
        case .message(let message):
            return message
        }
    }
}

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() {
        throw TestFailure.message(message)
    }
}

@main
struct GrooveMasterDomainTestRunner {
    static func main() {
        do {
            try testPracticeBlockKeepsMetronomeAndInspirationSeparate()
            try testGrooveEntryPersistsJournalIdentityWithoutScores()
            print("Groove Master domain tests passed.")
        } catch {
            fputs("Groove Master domain tests failed: \(error)\n", stderr)
            exit(1)
        }
    }
}

private func testPracticeBlockKeepsMetronomeAndInspirationSeparate() throws {
    let metronome = GroovePracticeBlock(
        id: "block_metronome",
        title: "Open E",
        durationSeconds: 300,
        source: .metronome(bpm: 80),
        allowedNotes: ["E"],
        focus: "Stable length and volume",
        challenge: "Make every note the same size",
        recordingPolicy: .optionalTrace,
        successReflection: "Did your body settle into time?"
    )
    let inspiration = GroovePracticeBlock(
        id: "block_reference",
        title: "One-note feel",
        durationSeconds: 600,
        source: .inspirationReference(
            GrooveInspirationReference(
                label: "Good Times",
                artist: "Chic",
                note: "Use one note to follow the feel."
            )
        ),
        allowedNotes: ["E"],
        focus: "Follow feel without learning the song",
        challenge: "Use fewer notes",
        recordingPolicy: .optionalTrace,
        successReflection: "Could you nod along without thinking?"
    )

    try expect(metronome.source.displayLabel == "80 BPM", "metronome block must expose BPM")
    try expect(
        inspiration.source.displayLabel == "Good Times",
        "inspiration block must expose the reference label"
    )
    try expect(
        inspiration.isSongLesson == false,
        "inspiration references must not become forced song lessons"
    )
}

private func testGrooveEntryPersistsJournalIdentityWithoutScores() throws {
    let entry = GrooveEntry(
        id: "groove_001",
        ordinal: 1,
        createdAt: Date(timeIntervalSince1970: 1_800_000_000),
        planID: "plan_001",
        phase: .feelTime,
        title: "Groove #001",
        challenge: "Leave more space",
        recording: GrooveRecordingTake(
            id: "take_001",
            fileURL: URL(fileURLWithPath: "/tmp/groove-001.m4a").absoluteString,
            durationSeconds: 900,
            captureMode: .room
        ),
        markers: [
            GrooveMarker(id: "marker_001", timestampSeconds: 128, note: "First relaxed pocket")
        ],
        reflection: GrooveReflection(tags: [.relaxed, .leftSpace], note: "Felt better after 5 min."),
        pocket: GroovePocketTrackerSnapshot(
            continuousPlaySeconds: 720,
            spaceUsage: .moreRoom,
            consistency: .stable,
            flowBreaks: 1,
            selectedMomentSeconds: 42
        ),
        producerNote: "You stayed with the same idea longer today."
    )

    let data = try JSONEncoder.grooveMaster.encode(entry)
    let decoded = try JSONDecoder.grooveMaster.decode(GrooveEntry.self, from: data)

    try expect(decoded.title == "Groove #001", "journal identity must survive JSON roundtrip")
    try expect(decoded.pocket.displayLines.contains("12m without stopping"), "pocket copy must be reflective")
    try expect(decoded.visibleText.joined(separator: "\n").contains("accuracy") == false, "entry must not expose scoring copy")
}
```

Create `clients/apple/scripts/test-groove-master-domain.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BUILD_DIR="${TMPDIR:-/tmp}/alan-groove-master-domain-tests"
MODULE_CACHE_DIR="$BUILD_DIR/clang-module-cache"
TEST_BINARY="$BUILD_DIR/groove-master-domain-tests"

rm -rf "$BUILD_DIR"
mkdir -p "$MODULE_CACHE_DIR"

CLANG_MODULE_CACHE_PATH="$MODULE_CACHE_DIR" swiftc \
    "$REPO_ROOT/clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterModels.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift" \
    "$REPO_ROOT/clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift" \
    "$REPO_ROOT/clients/apple/scripts/test-groove-master-domain.swift" \
    -o "$TEST_BINARY"

"$TEST_BINARY"
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: FAIL because the `Models/GrooveMaster` and `Services/GrooveMaster` files do not exist.

- [ ] **Step 3: Add domain model implementation**

Create `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterModels.swift`:

```swift
import Foundation

enum GroovePhase: String, Codable, CaseIterable, Equatable {
    case feelTime = "feel_time"
    case pocket
    case grooveConstruction = "groove_construction"
    case bassLanguage = "bass_language"
    case musicalConversation = "musical_conversation"

    var title: String {
        switch self {
        case .feelTime:
            return "Feel Time"
        case .pocket:
            return "Pocket"
        case .grooveConstruction:
            return "Groove Construction"
        case .bassLanguage:
            return "Bass Language"
        case .musicalConversation:
            return "Musical Conversation"
        }
    }
}

enum GrooveDayArchetype: String, Codable, CaseIterable, Equatable {
    case metronome
    case drumLoop = "drum_loop"
    case stealGroove = "steal_groove"
    case freeJam = "free_jam"
    case recording
    case deepListening = "deep_listening"
    case listeningOnly = "listening_only"
}

struct GrooveInspirationReference: Codable, Equatable {
    let label: String
    let artist: String?
    let note: String
}

struct GrooveLoopReference: Codable, Equatable {
    let id: String
    let title: String
    let style: String
    let bpm: Int
    let feel: String
    let energy: String
    let recommendedPhase: GroovePhase
    let fileURL: String?
}

enum GrooveSessionSource: Codable, Equatable {
    case metronome(bpm: Int)
    case drumLoop(GrooveLoopReference)
    case inspirationReference(GrooveInspirationReference)
    case silence

    private enum CodingKeys: String, CodingKey {
        case kind
        case bpm
        case loop
        case inspiration
    }

    private enum Kind: String, Codable {
        case metronome
        case drumLoop = "drum_loop"
        case inspirationReference = "inspiration_reference"
        case silence
    }

    var displayLabel: String {
        switch self {
        case .metronome(let bpm):
            return "\(bpm) BPM"
        case .drumLoop(let loop):
            return loop.title
        case .inspirationReference(let reference):
            return reference.label
        case .silence:
            return "Silence"
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Kind.self, forKey: .kind) {
        case .metronome:
            self = .metronome(bpm: try container.decode(Int.self, forKey: .bpm))
        case .drumLoop:
            self = .drumLoop(try container.decode(GrooveLoopReference.self, forKey: .loop))
        case .inspirationReference:
            self = .inspirationReference(
                try container.decode(GrooveInspirationReference.self, forKey: .inspiration)
            )
        case .silence:
            self = .silence
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .metronome(let bpm):
            try container.encode(Kind.metronome, forKey: .kind)
            try container.encode(bpm, forKey: .bpm)
        case .drumLoop(let loop):
            try container.encode(Kind.drumLoop, forKey: .kind)
            try container.encode(loop, forKey: .loop)
        case .inspirationReference(let inspiration):
            try container.encode(Kind.inspirationReference, forKey: .kind)
            try container.encode(inspiration, forKey: .inspiration)
        case .silence:
            try container.encode(Kind.silence, forKey: .kind)
        }
    }
}

enum GrooveRecordingPolicy: String, Codable, Equatable {
    case none
    case optionalTrace = "optional_trace"
    case selectedMinute = "selected_minute"
    case fullTake = "full_take"
}

struct GroovePracticeBlock: Codable, Equatable, Identifiable {
    let id: String
    let title: String
    let durationSeconds: Int
    let source: GrooveSessionSource
    let allowedNotes: [String]
    let focus: String
    let challenge: String
    let recordingPolicy: GrooveRecordingPolicy
    let successReflection: String

    var isSongLesson: Bool { false }
}

struct GrooveDayPlan: Codable, Equatable, Identifiable {
    let id: String
    let date: Date
    let journeyDay: Int
    let phase: GroovePhase
    let archetype: GrooveDayArchetype
    let title: String
    let challenge: String
    let blocks: [GroovePracticeBlock]

    var totalDurationSeconds: Int {
        blocks.reduce(0) { $0 + $1.durationSeconds }
    }
}

enum GrooveCaptureMode: String, Codable, Equatable {
    case room
    case cleanBass = "clean_bass"
    case multitrack
}

struct GrooveRecordingTake: Codable, Equatable, Identifiable {
    let id: String
    let fileURL: String
    let durationSeconds: Int
    let captureMode: GrooveCaptureMode
}

struct GrooveMarker: Codable, Equatable, Identifiable {
    let id: String
    let timestampSeconds: Int
    let note: String?
}

enum GrooveReflectionTag: String, Codable, CaseIterable, Equatable {
    case relaxed
    case tense
    case leftSpace = "left_space"
    case rushed
    case stayedWithIdea = "stayed_with_idea"
}

struct GrooveReflection: Codable, Equatable {
    let tags: [GrooveReflectionTag]
    let note: String
}

enum GrooveSpaceUsage: String, Codable, Equatable {
    case unknown
    case moreRoom = "more_room"
    case crowded
}

enum GrooveConsistency: String, Codable, Equatable {
    case unknown
    case settling
    case stable
}

struct GroovePocketTrackerSnapshot: Codable, Equatable {
    let continuousPlaySeconds: Int
    let spaceUsage: GrooveSpaceUsage
    let consistency: GrooveConsistency
    let flowBreaks: Int
    let selectedMomentSeconds: Int

    var displayLines: [String] {
        var lines: [String] = []
        if continuousPlaySeconds > 0 {
            lines.append("\(continuousPlaySeconds / 60)m without stopping")
        }
        switch spaceUsage {
        case .moreRoom:
            lines.append("You left more room today")
        case .crowded:
            lines.append("Try leaving more air next time")
        case .unknown:
            break
        }
        switch consistency {
        case .stable:
            lines.append("Your timing is becoming more stable")
        case .settling:
            lines.append("Your time started to settle")
        case .unknown:
            break
        }
        return lines
    }
}

struct GrooveEntry: Codable, Equatable, Identifiable {
    let id: String
    let ordinal: Int
    let createdAt: Date
    let planID: String
    let phase: GroovePhase
    let title: String
    let challenge: String
    let recording: GrooveRecordingTake?
    let markers: [GrooveMarker]
    let reflection: GrooveReflection
    let pocket: GroovePocketTrackerSnapshot
    let producerNote: String?

    var visibleText: [String] {
        var text = [title, phase.title, challenge]
        text.append(contentsOf: pocket.displayLines)
        if let producerNote {
            text.append(producerNote)
        }
        return text
    }
}

extension JSONEncoder {
    static var grooveMaster: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }
}

extension JSONDecoder {
    static var grooveMaster: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }
}
```

- [ ] **Step 4: Add temporary empty files needed by the compile script**

Create `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift`:

```swift
import Foundation
```

Create `clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift`:

```swift
import Foundation
```

Create `clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift`:

```swift
import Foundation
```

- [ ] **Step 5: Run domain tests**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: PASS with `Groove Master domain tests passed.`

- [ ] **Step 6: Commit**

```bash
git add \
  clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterModels.swift \
  clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift \
  clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift \
  clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift \
  clients/apple/scripts/test-groove-master-domain.swift \
  clients/apple/scripts/test-groove-master-domain.sh
git commit -m "Add Groove Master domain models"
```

---

### Task 2: Fixed Route Plan Engine

**Files:**
- Modify: `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift`
- Modify: `clients/apple/scripts/test-groove-master-domain.swift`

- [ ] **Step 1: Add failing plan-engine tests**

Insert these calls in `GrooveMasterDomainTestRunner.main()` after the existing tests:

```swift
try testPlanEngineGeneratesEarlyFeelTimeBlocks()
try testPlanEngineAllowsListeningOnlyAsPractice()
```

Append these tests to `clients/apple/scripts/test-groove-master-domain.swift`:

```swift
private func testPlanEngineGeneratesEarlyFeelTimeBlocks() throws {
    let start = Date(timeIntervalSince1970: 1_800_000_000)
    let plan = GrooveMasterPlanEngine.plan(
        for: start,
        journeyStart: start,
        history: .empty
    )

    try expect(plan.phase == .feelTime, "day 1 must start in Feel Time")
    try expect(plan.blocks.count == 3, "early Feel Time must include metronome, silence, and reference blocks")
    try expect(plan.blocks[0].source.displayLabel == "80 BPM", "first block must be 80 BPM metronome")
    try expect(plan.blocks[0].allowedNotes == ["E"], "early plan must restrict the user to open E")
    try expect(plan.blocks[1].source == .silence, "second block must train space explicitly")
    try expect(plan.totalDurationSeconds == 1200, "day 1 plan must total 20 minutes")
}

private func testPlanEngineAllowsListeningOnlyAsPractice() throws {
    let calendar = Calendar(identifier: .gregorian)
    let start = calendar.date(from: DateComponents(year: 2026, month: 6, day: 15))!
    let sunday = calendar.date(from: DateComponents(year: 2026, month: 6, day: 21))!
    let plan = GrooveMasterPlanEngine.plan(
        for: sunday,
        journeyStart: start,
        history: .empty
    )

    try expect(plan.archetype == .listeningOnly, "Sunday must be allowed as listening-only practice")
    try expect(plan.blocks.count == 1, "listening-only day must stay intentionally small")
    try expect(plan.blocks[0].recordingPolicy == .none, "listening-only day must not require recording")
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: FAIL because `GrooveMasterPlanEngine` and `GroovePracticeHistory` are missing.

- [ ] **Step 3: Implement deterministic plan engine**

Replace `clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift` with:

```swift
import Foundation

struct GroovePracticeHistory: Codable, Equatable {
    let completedEntryCount: Int
    let recentTags: [GrooveReflectionTag]
    let longestContinuousPlaySeconds: Int
    let skippedDays: Int

    static let empty = GroovePracticeHistory(
        completedEntryCount: 0,
        recentTags: [],
        longestContinuousPlaySeconds: 0,
        skippedDays: 0
    )
}

enum GrooveMasterPlanEngine {
    static func plan(
        for date: Date,
        journeyStart: Date,
        history: GroovePracticeHistory,
        calendar: Calendar = Calendar(identifier: .gregorian)
    ) -> GrooveDayPlan {
        let journeyDay = max(
            1,
            (calendar.dateComponents([.day], from: calendar.startOfDay(for: journeyStart), to: calendar.startOfDay(for: date)).day ?? 0) + 1
        )
        let phase = phase(forJourneyDay: journeyDay)
        let archetype = archetype(for: date, calendar: calendar)
        let blocks = blocks(for: phase, archetype: archetype, history: history)
        let challenge = challenge(for: phase, archetype: archetype, history: history)

        return GrooveDayPlan(
            id: "plan_\(Self.isoDayFormatter.string(from: date))",
            date: date,
            journeyDay: journeyDay,
            phase: phase,
            archetype: archetype,
            title: "\(phase.title) · \(archetypeTitle(archetype))",
            challenge: challenge,
            blocks: blocks
        )
    }

    static func phase(forJourneyDay day: Int) -> GroovePhase {
        switch day {
        case 1...28:
            return .feelTime
        case 29...56:
            return .pocket
        case 57...84:
            return .grooveConstruction
        case 85...180:
            return .bassLanguage
        default:
            return .musicalConversation
        }
    }

    private static func archetype(for date: Date, calendar: Calendar) -> GrooveDayArchetype {
        switch calendar.component(.weekday, from: date) {
        case 2:
            return .metronome
        case 3:
            return .drumLoop
        case 4:
            return .stealGroove
        case 5:
            return .freeJam
        case 6:
            return .recording
        case 7:
            return .deepListening
        default:
            return .listeningOnly
        }
    }

    private static func blocks(
        for phase: GroovePhase,
        archetype: GrooveDayArchetype,
        history: GroovePracticeHistory
    ) -> [GroovePracticeBlock] {
        if archetype == .listeningOnly {
            return [
                GroovePracticeBlock(
                    id: "listen",
                    title: "Listening Only",
                    durationSeconds: 1200,
                    source: .inspirationReference(Self.defaultInspiration),
                    allowedNotes: [],
                    focus: "Listen for space and repetition",
                    challenge: "Do not play today",
                    recordingPolicy: .none,
                    successReflection: "What made your body move?"
                )
            ]
        }

        switch phase {
        case .feelTime:
            return [
                GroovePracticeBlock(
                    id: "open_e_metronome",
                    title: "Open E",
                    durationSeconds: 300,
                    source: .metronome(bpm: 80),
                    allowedNotes: ["E"],
                    focus: "Stable length and volume",
                    challenge: "Make every note the same size",
                    recordingPolicy: .optionalTrace,
                    successReflection: "Did your body settle into time?"
                ),
                GroovePracticeBlock(
                    id: "open_e_space",
                    title: "E + Silence",
                    durationSeconds: 300,
                    source: .silence,
                    allowedNotes: ["E"],
                    focus: "Feel the gap after the note",
                    challenge: "Let the rest be part of the groove",
                    recordingPolicy: .optionalTrace,
                    successReflection: "Could you feel the silence?"
                ),
                GroovePracticeBlock(
                    id: "one_note_reference",
                    title: "One-note Feel",
                    durationSeconds: 600,
                    source: .inspirationReference(Self.defaultInspiration),
                    allowedNotes: ["E"],
                    focus: "Follow feel without learning the song",
                    challenge: "Use fewer notes",
                    recordingPolicy: .optionalTrace,
                    successReflection: "Could you nod along without thinking?"
                ),
            ]
        case .pocket:
            return [
                notePaletteBlock(id: "e_b_octave", title: "E · B · high E", seconds: 600),
                loopBlock(id: "funk_loop_jam", title: "Funk Loop Jam", seconds: 600),
            ]
        case .grooveConstruction:
            return [
                loopBlock(id: "repeat_variation", title: "Repeat + Vary", seconds: 900),
                recordingBlock(id: "selected_minute", title: "Record One Minute", seconds: 60),
            ]
        case .bassLanguage:
            return [
                notePaletteBlock(id: "root_fifth_octave", title: "Root · Fifth · Octave", seconds: 600),
                recordingBlock(id: "four_bar_loop", title: "Four-bar Loop", seconds: 600),
            ]
        case .musicalConversation:
            return [
                loopBlock(id: "conversation", title: "Talk With The Drums", seconds: 1200),
            ]
        }
    }

    private static func notePaletteBlock(id: String, title: String, seconds: Int) -> GroovePracticeBlock {
        GroovePracticeBlock(
            id: id,
            title: title,
            durationSeconds: seconds,
            source: .metronome(bpm: 90),
            allowedNotes: ["E", "B", "E↑"],
            focus: "Keep the phrase relaxed",
            challenge: "Do not stop",
            recordingPolicy: .optionalTrace,
            successReflection: "Could you continue without thinking?"
        )
    }

    private static func loopBlock(id: String, title: String, seconds: Int) -> GroovePracticeBlock {
        GroovePracticeBlock(
            id: id,
            title: title,
            durationSeconds: seconds,
            source: .drumLoop(Self.defaultLoop),
            allowedNotes: ["E", "B", "E↑"],
            focus: "Stay with the drums",
            challenge: "Leave more space",
            recordingPolicy: .optionalTrace,
            successReflection: "Did the groove keep moving?"
        )
    }

    private static func recordingBlock(id: String, title: String, seconds: Int) -> GroovePracticeBlock {
        GroovePracticeBlock(
            id: id,
            title: title,
            durationSeconds: seconds,
            source: .drumLoop(Self.defaultLoop),
            allowedNotes: ["E", "B", "E↑"],
            focus: "Capture one idea",
            challenge: "Repeat it longer",
            recordingPolicy: .selectedMinute,
            successReflection: "Would you listen to this loop again?"
        )
    }

    private static func challenge(
        for phase: GroovePhase,
        archetype: GrooveDayArchetype,
        history: GroovePracticeHistory
    ) -> String {
        if history.recentTags.contains(.tense) {
            return "Stay relaxed"
        }
        switch archetype {
        case .metronome:
            return "Make every note the same size"
        case .drumLoop:
            return "Leave more space"
        case .stealGroove:
            return "Steal the rhythm, not the song"
        case .freeJam:
            return "Stay with one idea longer"
        case .recording:
            return "Capture one minute"
        case .deepListening:
            return "Notice where the bass stops"
        case .listeningOnly:
            return "Only listen"
        }
    }

    private static func archetypeTitle(_ archetype: GrooveDayArchetype) -> String {
        switch archetype {
        case .metronome:
            return "Metronome"
        case .drumLoop:
            return "Drum Loop"
        case .stealGroove:
            return "Steal Groove"
        case .freeJam:
            return "Free Jam"
        case .recording:
            return "Recording"
        case .deepListening:
            return "Deep Listening"
        case .listeningOnly:
            return "Listening Only"
        }
    }

    private static let defaultInspiration = GrooveInspirationReference(
        label: "Good Times",
        artist: "Chic",
        note: "Use one note to follow the feel."
    )

    private static let defaultLoop = GrooveLoopReference(
        id: "built_in_funk_90",
        title: "Built-in Funk 90",
        style: "Funk",
        bpm: 90,
        feel: "straight",
        energy: "medium",
        recommendedPhase: .pocket,
        fileURL: nil
    )

    private static let isoDayFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyyMMdd"
        return formatter
    }()
}
```

- [ ] **Step 4: Run domain tests**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: PASS with `Groove Master domain tests passed.`

- [ ] **Step 5: Commit**

```bash
git add \
  clients/apple/alan-macos/Models/GrooveMaster/GrooveMasterPlanEngine.swift \
  clients/apple/scripts/test-groove-master-domain.swift
git commit -m "Add Groove Master plan engine"
```

---

### Task 3: Journal Store And Fake Audio Capture

**Files:**
- Modify: `clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift`
- Modify: `clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift`
- Modify: `clients/apple/scripts/test-groove-master-domain.swift`

- [ ] **Step 1: Add failing persistence and capture tests**

Insert these calls in `GrooveMasterDomainTestRunner.main()`:

```swift
try testJournalStorePersistsEntriesInOrder()
try testFakeAudioCaptureRecordsMarkersWithoutMicrophone()
```

Append these tests:

```swift
private func testJournalStorePersistsEntriesInOrder() throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("groove-journal-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let store = GrooveMasterJournalStore(rootDirectory: root)
    let first = sampleEntry(id: "groove_001", ordinal: 1)
    let second = sampleEntry(id: "groove_002", ordinal: 2)

    try store.save(first)
    try store.save(second)

    let entries = try store.loadEntries()
    try expect(entries.map(\.id) == ["groove_001", "groove_002"], "journal store must load entries by ordinal")
    try expect(
        FileManager.default.fileExists(atPath: root.appendingPathComponent("entries/groove_001.json").path),
        "journal entry must persist as an inspectable JSON file"
    )
}

private func testFakeAudioCaptureRecordsMarkersWithoutMicrophone() throws {
    let capture = FakeGrooveAudioCaptureClient()
    let take = try capture.startRoomCapture(sessionID: "session_001")
    try expect(take.captureMode == .room, "fake capture must use room mode")

    try capture.mark(timestampSeconds: 42, note: "Pocket settled")
    let completed = try capture.stop()

    try expect(completed.markers.map(\.timestampSeconds) == [42], "fake capture must preserve markers")
    try expect(completed.take.id == take.id, "completed capture must use the started take")
}

private func sampleEntry(id: String, ordinal: Int) -> GrooveEntry {
    GrooveEntry(
        id: id,
        ordinal: ordinal,
        createdAt: Date(timeIntervalSince1970: 1_800_000_000 + Double(ordinal)),
        planID: "plan_001",
        phase: .feelTime,
        title: "Groove #\(String(format: "%03d", ordinal))",
        challenge: "Leave more space",
        recording: nil,
        markers: [],
        reflection: GrooveReflection(tags: [.relaxed], note: "Felt good."),
        pocket: GroovePocketTrackerSnapshot(
            continuousPlaySeconds: 600,
            spaceUsage: .moreRoom,
            consistency: .settling,
            flowBreaks: 0,
            selectedMomentSeconds: 30
        ),
        producerNote: nil
    )
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: FAIL because `GrooveMasterJournalStore`, `FakeGrooveAudioCaptureClient`, and `CompletedGrooveCapture` are missing.

- [ ] **Step 3: Implement journal store**

Replace `clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift` with:

```swift
import Foundation

enum GrooveMasterJournalStoreError: Error, Equatable {
    case entryEncodingFailed(String)
}

struct GrooveMasterJournalStore {
    let rootDirectory: URL
    let fileManager: FileManager

    init(rootDirectory: URL, fileManager: FileManager = .default) {
        self.rootDirectory = rootDirectory
        self.fileManager = fileManager
    }

    var entriesDirectory: URL {
        rootDirectory.appendingPathComponent("entries", isDirectory: true)
    }

    func save(_ entry: GrooveEntry) throws {
        try fileManager.createDirectory(at: entriesDirectory, withIntermediateDirectories: true)
        let url = entriesDirectory.appendingPathComponent("\(entry.id).json")
        let data = try JSONEncoder.grooveMaster.encode(entry)
        try data.write(to: url, options: [.atomic])
    }

    func loadEntries() throws -> [GrooveEntry] {
        guard fileManager.fileExists(atPath: entriesDirectory.path) else {
            return []
        }
        let urls = try fileManager.contentsOfDirectory(
            at: entriesDirectory,
            includingPropertiesForKeys: nil
        )
        .filter { $0.pathExtension == "json" }

        let entries = try urls.map { url in
            let data = try Data(contentsOf: url)
            return try JSONDecoder.grooveMaster.decode(GrooveEntry.self, from: data)
        }
        return entries.sorted { lhs, rhs in
            if lhs.ordinal == rhs.ordinal {
                return lhs.createdAt < rhs.createdAt
            }
            return lhs.ordinal < rhs.ordinal
        }
    }
}
```

- [ ] **Step 4: Implement fake capture client**

Replace `clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift` with:

```swift
import Foundation

struct CompletedGrooveCapture: Equatable {
    let take: GrooveRecordingTake
    let markers: [GrooveMarker]
}

protocol GrooveAudioCaptureClient {
    func startRoomCapture(sessionID: String) throws -> GrooveRecordingTake
    func mark(timestampSeconds: Int, note: String?) throws
    func stop() throws -> CompletedGrooveCapture
}

final class FakeGrooveAudioCaptureClient: GrooveAudioCaptureClient {
    private var activeTake: GrooveRecordingTake?
    private var markers: [GrooveMarker] = []

    func startRoomCapture(sessionID: String) throws -> GrooveRecordingTake {
        let take = GrooveRecordingTake(
            id: "take_\(sessionID)",
            fileURL: URL(fileURLWithPath: "/tmp/\(sessionID).m4a").absoluteString,
            durationSeconds: 0,
            captureMode: .room
        )
        activeTake = take
        markers = []
        return take
    }

    func mark(timestampSeconds: Int, note: String?) throws {
        markers.append(
            GrooveMarker(
                id: "marker_\(markers.count + 1)",
                timestampSeconds: timestampSeconds,
                note: note
            )
        )
    }

    func stop() throws -> CompletedGrooveCapture {
        let take = activeTake ?? GrooveRecordingTake(
            id: "take_inactive",
            fileURL: URL(fileURLWithPath: "/tmp/inactive.m4a").absoluteString,
            durationSeconds: 0,
            captureMode: .room
        )
        activeTake = nil
        return CompletedGrooveCapture(take: take, markers: markers)
    }
}
```

- [ ] **Step 5: Run domain tests**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: PASS with `Groove Master domain tests passed.`

- [ ] **Step 6: Commit**

```bash
git add \
  clients/apple/alan-macos/Services/GrooveMaster/GrooveMasterJournalStore.swift \
  clients/apple/alan-macos/Services/GrooveMaster/GrooveAudioCaptureClient.swift \
  clients/apple/scripts/test-groove-master-domain.swift
git commit -m "Add Groove Master journal persistence"
```

---

### Task 4: Shell Content Kind And Descriptor Routing

**Files:**
- Modify: `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift`
- Modify: `clients/apple/alan-macos/Services/Shell/ShellContentRenderingRegistry.swift`
- Modify: `clients/apple/scripts/test-shell-runtime-metadata.swift`

- [ ] **Step 1: Add failing shell content tests**

In `verifiesContentRenderingRegistryRoutesSupportedKinds()`, add this content next to markdown/settings:

```swift
let groove = ShellContentInstance(
    contentID: ShellContentInstance.grooveMasterContentID,
    kind: .grooveMaster,
    title: "Groove Master",
    payload: .grooveMaster(
        ShellGrooveMasterContentPayload(
            surfaceID: ShellContentInstance.grooveMasterSurfaceID,
            title: "Groove Master"
        )
    )
)
```

After the settings descriptor expectations, add:

```swift
let grooveDescriptor = ShellContentRenderingRegistry.descriptor(for: groove)
expect(grooveDescriptor.renderKind == .grooveMaster, "Groove Master content must route to Groove renderer")
expect(grooveDescriptor.iconName == "music.note", "Groove Master descriptor must get a music icon")
expect(
    grooveDescriptor.capabilities == [.grooveMasterSurface],
    "Groove Master descriptor must expose only Groove Master surface capability"
)
expect(
    !grooveDescriptor.capabilities.contains(.terminalInput),
    "Groove Master content must not expose terminal input"
)
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: FAIL because `.grooveMaster`, `.grooveMasterSurface`, and payload support do not exist.

- [ ] **Step 3: Add Groove Master shell payload types**

Modify `clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift`:

Add to `ShellContentKind`:

```swift
case grooveMaster = "groove_master"
```

Add to `ShellContentIntent`:

```swift
case grooveMaster(title: String?)
```

Add to `ShellContentCapability`:

```swift
case grooveMasterSurface = "groove_master_surface"
```

Add after `ShellSettingsContentPayload`:

```swift
struct ShellGrooveMasterContentPayload: Codable, Equatable {
    let surfaceID: String
    let title: String?

    private enum CodingKeys: String, CodingKey {
        case surfaceID = "surface_id"
        case title
    }
}
```

Add a property to `ShellContentPayload`:

```swift
let grooveMaster: ShellGrooveMasterContentPayload?
```

Update `ShellContentPayload.CodingKeys`:

```swift
case grooveMaster = "groove_master"
```

Update the three existing factory methods to pass `grooveMaster: nil`, and add:

```swift
static func grooveMaster(_ payload: ShellGrooveMasterContentPayload) -> ShellContentPayload {
    ShellContentPayload(terminal: nil, markdown: nil, settings: nil, grooveMaster: payload)
}
```

Update `ShellContentInstance.defaultCapabilities(for:)`:

```swift
case .grooveMaster:
    return [.grooveMasterSurface]
```

Add to `extension ShellContentInstance` near settings constants:

```swift
static let grooveMasterSurfaceID = "groove_master_main"
static let grooveMasterContentID = "content_groove_master_main"
```

- [ ] **Step 4: Route renderer descriptors**

Modify `clients/apple/alan-macos/Services/Shell/ShellContentRenderingRegistry.swift`:

Add to `ShellContentRenderKind`:

```swift
case grooveMaster = "groove_master"
```

Add to `renderKind(for:)`:

```swift
case .grooveMaster:
    return .grooveMaster
```

Add to `iconName(for:)`:

```swift
case .grooveMaster:
    return "music.note"
```

- [ ] **Step 5: Run shell runtime metadata tests**

Run:

```bash
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: PASS.

- [ ] **Step 6: Run domain tests**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add \
  clients/apple/alan-macos/Models/Shell/ShellSnapshots.swift \
  clients/apple/alan-macos/Services/Shell/ShellContentRenderingRegistry.swift \
  clients/apple/scripts/test-shell-runtime-metadata.swift
git commit -m "Register Groove Master shell content"
```

---

### Task 5: Open Groove Master Content Tab

**Files:**
- Modify: `clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift`
- Modify: `clients/apple/alan-macos/ShellHostController.swift`
- Modify: `clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift`
- Modify: `clients/apple/scripts/test-shell-runtime-metadata.swift`

- [ ] **Step 1: Add failing open-tab test**

Append this test near the markdown/settings open tests in `clients/apple/scripts/test-shell-runtime-metadata.swift`:

```swift
private static func verifiesOpeningGrooveMasterTabCreatesSingletonShellContent() {
    let controller = makeController()
    let initialTabCount = controller.shellState.spaces.flatMap(\.tabs).count
    guard let firstTabID = controller.openGrooveMasterTab() else {
        fail("opening Groove Master must create a shell tab")
    }

    let firstProjection = controller.shellState.contentStateProjection()
    guard let content = firstProjection.focusedContent else {
        fail("Groove Master tab must focus a content descriptor")
    }
    let descriptor = ShellContentRenderingRegistry.descriptor(for: content)
    let selectedPaneID = controller.selectedPane?.paneID

    expect(controller.shellState.focusedTabID == firstTabID, "Groove Master open must focus the tab")
    expect(content.contentID == ShellContentInstance.grooveMasterContentID, "Groove Master content must use canonical content ID")
    expect(content.kind == .grooveMaster, "Groove Master open must create Groove content")
    expect(content.title == "Groove Master", "Groove Master content must expose product title")
    expect(
        content.payload.grooveMaster?.surfaceID == ShellContentInstance.grooveMasterSurfaceID,
        "Groove Master descriptor must persist canonical surface identity"
    )
    expect(content.capabilities == [.grooveMasterSurface], "Groove Master content must expose only Groove surface capability")
    expect(descriptor.renderKind == .grooveMaster, "Groove Master descriptor must route to Groove renderer")
    expect(
        controller.selectedPane?.launchTarget == nil && controller.selectedPane?.process == nil,
        "Groove Master pane must not describe a terminal process"
    )
    expect(
        selectedPaneID.map { !controller.terminalRuntimeRegistry.registeredPaneIDs.contains($0) } == true,
        "Groove Master open must not create a terminal runtime"
    )

    guard let secondTabID = controller.openGrooveMasterTab() else {
        fail("reopening Groove Master must focus the existing tab")
    }
    let secondProjection = controller.shellState.contentStateProjection()
    let grooveContents = secondProjection.contents.filter { $0.kind == .grooveMaster }

    expect(secondTabID == firstTabID, "reopening Groove Master must return existing tab")
    expect(controller.shellState.spaces.flatMap(\.tabs).count == initialTabCount + 1, "reopening Groove Master must not duplicate tabs")
    expect(grooveContents.count == 1, "Groove Master content must remain singleton")
}
```

Add `try` or direct call in the test runner where other static checks are invoked:

```swift
verifiesOpeningGrooveMasterTabCreatesSingletonShellContent()
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: FAIL because `openGrooveMasterTab()` and `.grooveMaster` content intent mounting are missing.

- [ ] **Step 3: Implement content mounting**

In `ShellStateMutations.swift`, add a `.grooveMaster` case to `prepareContentMount`:

```swift
case .grooveMaster(let title):
    let resolvedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        ? title!.trimmingCharacters(in: .whitespacesAndNewlines)
        : "Groove Master"
    let pane = makeContentPlaceholderPane(
        paneID: paneID,
        tabID: tabID,
        spaceID: spaceID,
        title: resolvedTitle,
        summary: "Groove Master ready",
        now: now
    )
    let paneSlot = ShellPaneSlot(
        paneSlotID: paneID,
        tabID: tabID,
        spaceID: spaceID,
        contentID: ShellContentInstance.grooveMasterContentID,
        attention: .active
    )
    let content = ShellContentInstance(
        contentID: ShellContentInstance.grooveMasterContentID,
        kind: .grooveMaster,
        title: resolvedTitle,
        payload: .grooveMaster(
            ShellGrooveMasterContentPayload(
                surfaceID: ShellContentInstance.grooveMasterSurfaceID,
                title: resolvedTitle
            )
        ),
        rendererState: ShellContentRendererState(
            phase: "ready",
            detail: ShellContentInstance.grooveMasterSurfaceID
        )
    )
    return ShellPreparedContentMount(
        pane: pane,
        paneSlot: paneSlot,
        content: content,
        title: resolvedTitle
    )
```

In the singleton reuse logic that currently handles `.settings`, add Groove Master with the same singleton behavior:

```swift
if case .some(.grooveMaster) = contentIntent,
   let existing = firstPaneSlotMountingContent(where: { content in
       content.kind == .grooveMaster
           && content.contentID == ShellContentInstance.grooveMasterContentID
           && content.payload.grooveMaster?.surfaceID == ShellContentInstance.grooveMasterSurfaceID
   }) {
    return existing
}
```

- [ ] **Step 4: Add host API**

In `ShellHostController.swift`, add next to `openSettingsTab`:

```swift
@discardableResult
func openGrooveMasterTab(
    in spaceID: String? = nil,
    title: String? = nil
) -> String? {
    openContentTab(
        .grooveMaster(title: title),
        in: spaceID
    )
}
```

- [ ] **Step 5: Add sidebar label**

In `ShellSidebarView.swift`, extend content kind labeling:

```swift
case .grooveMaster:
    return "Groove"
```

If the file has icon logic for markdown/settings, add:

```swift
case .grooveMaster:
    return "music.note"
```

- [ ] **Step 6: Run shell runtime metadata tests**

Run:

```bash
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add \
  clients/apple/alan-macos/Models/Shell/ShellStateMutations.swift \
  clients/apple/alan-macos/ShellHostController.swift \
  clients/apple/alan-macos/Views/Shell/ShellSidebarView.swift \
  clients/apple/scripts/test-shell-runtime-metadata.swift
git commit -m "Open Groove Master as shell content"
```

---

### Task 6: Groove Master SwiftUI Surface

**Files:**
- Create: `clients/apple/alan-macos/Views/GrooveMaster/GrooveMasterContentView.swift`
- Modify: `clients/apple/alan-macos/TerminalPaneView.swift`

- [ ] **Step 1: Add renderer dispatch expectation**

In `TerminalPaneView.swift`, update `ShellBoundedContentLeafView.body` switch to prepare for `.grooveMaster`. This step should fail to compile until the view exists.

Add:

```swift
case .grooveMaster:
    GrooveMasterContentView(descriptor: descriptor)
        .contentShape(Rectangle())
        .onTapGesture(perform: onFocusPane)
```

Add to `contentKindLabel`:

```swift
case .grooveMaster:
    return "Groove"
```

Run:

```bash
xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -destination generic/platform=macOS -derivedDataPath /tmp/alan-groove-master-build CODE_SIGNING_ALLOWED=NO build
```

Expected: FAIL because `GrooveMasterContentView` does not exist.

- [ ] **Step 2: Implement first surface**

Create `clients/apple/alan-macos/Views/GrooveMaster/GrooveMasterContentView.swift`:

```swift
import SwiftUI

#if os(macOS)
struct GrooveMasterContentView: View {
    let descriptor: ShellContentRenderDescriptor
    @State private var plan = GrooveMasterPlanEngine.plan(
        for: Date(),
        journeyStart: Date(),
        history: .empty
    )
    @State private var isSessionActive = false
    @State private var elapsedSeconds = 0
    @State private var markers: [GrooveMarker] = []
    @State private var reflectionText = ""

    var body: some View {
        ZStack {
            GrooveMasterPalette.background

            VStack(alignment: .leading, spacing: 18) {
                header
                todayPlan
                blockList
                sessionControls
                pocketPreview
                Spacer(minLength: 0)
            }
            .padding(28)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Groove Master")
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(GrooveMasterPalette.primaryInk)
            Text("Stop Learning Songs. Start Feeling Groove.")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(GrooveMasterPalette.secondaryInk)
        }
    }

    private var todayPlan: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Today")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(GrooveMasterPalette.accent)
            Text(plan.title)
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(GrooveMasterPalette.primaryInk)
            Text("\(plan.totalDurationSeconds / 60) min · \(plan.challenge)")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(GrooveMasterPalette.secondaryInk)
        }
    }

    private var blockList: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(plan.blocks) { block in
                HStack(spacing: 12) {
                    Text("\(block.durationSeconds / 60)m")
                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                        .foregroundStyle(GrooveMasterPalette.accent)
                        .frame(width: 42, alignment: .leading)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(block.title)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(GrooveMasterPalette.primaryInk)
                        Text("\(block.source.displayLabel) · \(block.focus)")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(GrooveMasterPalette.secondaryInk)
                            .lineLimit(2)
                    }
                }
                .padding(.vertical, 8)
            }
        }
    }

    private var sessionControls: some View {
        HStack(spacing: 10) {
            Button(isSessionActive ? "End Session" : "Start Session") {
                if isSessionActive {
                    isSessionActive = false
                } else {
                    isSessionActive = true
                    elapsedSeconds = 0
                    markers = []
                }
            }
            .buttonStyle(.borderedProminent)

            Button("Mark Moment") {
                markers.append(
                    GrooveMarker(
                        id: "marker_\(markers.count + 1)",
                        timestampSeconds: elapsedSeconds,
                        note: "Marked moment"
                    )
                )
            }
            .disabled(!isSessionActive)

            Text("\(elapsedSeconds / 60)m \(elapsedSeconds % 60)s")
                .font(.system(size: 12, weight: .semibold, design: .monospaced))
                .foregroundStyle(GrooveMasterPalette.secondaryInk)
        }
    }

    private var pocketPreview: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Pocket Tracker")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(GrooveMasterPalette.accent)
            Text(markers.isEmpty ? "Mark one moment when the pocket settles." : "\(markers.count) marked moment\(markers.count == 1 ? "" : "s")")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(GrooveMasterPalette.secondaryInk)
        }
        .padding(.top, 8)
    }
}

private enum GrooveMasterPalette {
    static let background = LinearGradient(
        colors: [
            Color(red: 0.05, green: 0.04, blue: 0.10),
            Color(red: 0.09, green: 0.08, blue: 0.18),
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
    static let primaryInk = Color(red: 0.92, green: 0.94, blue: 0.90)
    static let secondaryInk = Color(red: 0.62, green: 0.65, blue: 0.72)
    static let accent = Color(red: 0.62, green: 0.96, blue: 0.31)
}
#endif
```

- [ ] **Step 3: Run build**

Run:

```bash
xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -destination generic/platform=macOS -derivedDataPath /tmp/alan-groove-master-build CODE_SIGNING_ALLOWED=NO build
```

Expected: PASS.

- [ ] **Step 4: Run focused shell tests**

Run:

```bash
bash clients/apple/scripts/test-shell-runtime-metadata.sh
bash clients/apple/scripts/test-groove-master-domain.sh
```

Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add \
  clients/apple/alan-macos/Views/GrooveMaster/GrooveMasterContentView.swift \
  clients/apple/alan-macos/TerminalPaneView.swift
git commit -m "Add Groove Master content surface"
```

---

### Task 7: App Command To Open Groove Master

**Files:**
- Modify: `clients/apple/alan-macos/App/AlanMacShellCommands.swift`
- Modify: `clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift`
- Modify: `clients/apple/scripts/test-shell-action-registry.swift`

- [ ] **Step 1: Add failing action-registry test**

In `clients/apple/scripts/test-shell-action-registry.swift`, add a test asserting the registry includes an action with id `openGrooveMaster` and title `Open Groove Master`.

Use this assertion body:

```swift
let registry = ShellActionRegistry.defaultRegistry
let action = registry.action(for: .openGrooveMaster)
try expect(action?.title == "Open Groove Master", "Groove Master action must be registered")
try expect(action?.category == .workspace, "Groove Master action must live with workspace opening actions")
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
bash clients/apple/scripts/test-shell-action-registry.sh
```

Expected: FAIL because `.openGrooveMaster` is missing.

- [ ] **Step 3: Add shell action**

In `ShellActionRegistry.swift`, add enum case:

```swift
case openGrooveMaster
```

Register it in `defaultRegistry`:

```swift
ShellAction(
    id: .openGrooveMaster,
    title: "Open Groove Master",
    category: .workspace,
    defaultShortcut: nil
)
```

In shell action execution, route:

```swift
case .openGrooveMaster:
    openGrooveMasterTab()
```

- [ ] **Step 4: Add menu command**

In `AlanMacShellCommands.swift`, add a menu button where other workspace open commands live:

```swift
Button("Open Groove Master") {
    owner.shellHostController?.openGrooveMasterTab()
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
bash clients/apple/scripts/test-shell-action-registry.sh
bash clients/apple/scripts/test-shell-runtime-metadata.sh
```

Expected: both PASS.

- [ ] **Step 6: Commit**

```bash
git add \
  clients/apple/alan-macos/App/AlanMacShellCommands.swift \
  clients/apple/alan-macos/Models/Shell/ShellActionRegistry.swift \
  clients/apple/scripts/test-shell-action-registry.swift
git commit -m "Add Groove Master open command"
```

---

### Task 8: Full Verification

**Files:**
- No source edits unless verification reveals a defect.

- [ ] **Step 1: Run OpenSpec validation**

Run:

```bash
openspec validate define-groove-master-environment-app --strict
```

Expected: PASS with `Change 'define-groove-master-environment-app' is valid`.

- [ ] **Step 2: Run focused script tests**

Run:

```bash
bash clients/apple/scripts/test-groove-master-domain.sh
bash clients/apple/scripts/test-shell-runtime-metadata.sh
bash clients/apple/scripts/test-shell-action-registry.sh
```

Expected: all PASS.

- [ ] **Step 3: Run macOS build**

Run:

```bash
xcodebuild -project clients/apple/alan-macos.xcodeproj -scheme alan-macos -configuration Debug -destination generic/platform=macOS -derivedDataPath /tmp/alan-groove-master-build CODE_SIGNING_ALLOWED=NO build
```

Expected: PASS.

- [ ] **Step 4: Check whitespace**

Run:

```bash
git diff --check
```

Expected: no output.

- [ ] **Step 5: Final commit if verification fixes were needed**

If verification required source changes, stage the specific files printed by
`git status --short`. For example, when only the Groove Master content view and
domain test changed:

```bash
git add \
  clients/apple/alan-macos/Views/GrooveMaster/GrooveMasterContentView.swift \
  clients/apple/scripts/test-groove-master-domain.swift
git commit -m "Fix Groove Master verification"
```

If no source changes were needed, do not create an empty commit.

## Self-Review Notes

- Spec coverage:
  - Product boundary: Tasks 4-7 expose Groove Master as non-terminal environment content while keeping domain files separate.
  - Daily loop: Tasks 1-3 model Today Plan, blocks, journal, markers, reflection, and pocket signals.
  - Practice route: Task 2 implements the fixed 12-month route and weekly cadence.
  - Audio model: Task 3 adds capture interfaces and fake room capture; real AVFoundation can replace the client without changing domain tests.
  - Inspiration and Pocket Tracker: Tasks 1-2 model references, non-song lessons, and reflective tracker copy.
  - Alan adapter: Tasks 4-7 wire shell content kind, payload, renderer, open method, UI dispatch, and command.
- Deferred scope:
  - Payments, community, mobile capture, clean bass capture, multitrack, deep analysis, sync, and full DAW behavior are not implemented.
- Placeholder scan:
  - No plan steps use open-ended placeholder wording.
