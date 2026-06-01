import Foundation

#if os(macOS)
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
struct ShellPerformanceDiagnosticsTestRunner {
    static func main() {
        do {
            try testDiagnosticsAreDefaultOffAndClearOnDisable()
            try testControllerPersistsPreferenceAndClearsOnDisable()
            try testCurrentProcessSamplerProducesNumericAlanSample()
            try testKnownTerminalChildAggregateSamplingOmitsCommandIdentity()
            try testDescendantProcessSamplerAggregatesChildPressure()
            try testBoundedRingBufferAndAutomaticStutterMarker()
            try testProcessSamplesStayBounded()
            try testExportBundleOmitsSensitiveFixtureStrings()
            print("Shell performance diagnostics tests passed.")
        } catch {
            fputs("Shell performance diagnostics tests failed: \(error)\n", stderr)
            exit(1)
        }
    }
}

private func testDiagnosticsAreDefaultOffAndClearOnDisable() throws {
    let recorder = AlanPerformanceDiagnosticsRecorder(
        configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 8)
    )

    recorder.record(
        AlanPerformanceDiagnosticEvent(
            kind: .ghosttyWakeup,
            durationMs: 2,
            paneID: "pane_1",
            contentID: "terminal_pane_1",
            priority: "foregroundInteractive",
            visibility: "visible",
            thread: "main"
        )
    )
    try expect(recorder.eventsSnapshot().isEmpty, "disabled diagnostics must not record events")

    recorder.setEnabled(true)
    recorder.record(
        AlanPerformanceDiagnosticEvent(
            kind: .ghosttyWakeup,
            durationMs: 2,
            paneID: "pane_1",
            contentID: "terminal_pane_1",
            priority: "foregroundInteractive",
            visibility: "visible",
            thread: "main"
        )
    )
    try expect(recorder.eventsSnapshot().count == 1, "enabled diagnostics must record events")

    recorder.setEnabled(false)
    try expect(recorder.eventsSnapshot().isEmpty, "disabling diagnostics must clear unexported events")
}

private func testControllerPersistsPreferenceAndClearsOnDisable() throws {
    let suiteName = "alan-performance-diagnostics-\(UUID().uuidString)"
    guard let defaults = UserDefaults(suiteName: suiteName) else {
        throw TestFailure.message("could not create isolated user defaults")
    }
    defer {
        defaults.removePersistentDomain(forName: suiteName)
    }

    let recorder = AlanPerformanceDiagnosticsRecorder(
        configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 4)
    )
    let controller = AlanPerformanceDiagnosticsController(defaults: defaults, recorder: recorder)

    try expect(!controller.isEnabled, "diagnostics controller must default to disabled")
    try expect(!defaults.bool(forKey: AlanPerformanceDiagnosticsController.preferenceKey),
               "diagnostics preference must default to false")

    controller.setEnabled(true)
    try expect(controller.isEnabled, "controller must enable diagnostics")
    try expect(defaults.bool(forKey: AlanPerformanceDiagnosticsController.preferenceKey),
               "controller must persist enabled preference")

    controller.record(.shellRuntimeProjection, durationMs: 4)
    try expect(recorder.eventsSnapshot().count == 1, "controller must route events while enabled")

    controller.setEnabled(false)
    try expect(!controller.isEnabled, "controller must disable diagnostics")
    try expect(!defaults.bool(forKey: AlanPerformanceDiagnosticsController.preferenceKey),
               "controller must persist disabled preference")
    try expect(recorder.eventsSnapshot().isEmpty, "controller disable must clear unexported buffers")
}

private func testCurrentProcessSamplerProducesNumericAlanSample() throws {
    let sample = AlanPerformanceProcessSampler.currentAlanProcessSample()

    try expect(
        sample.processID == ProcessInfo.processInfo.processIdentifier,
        "process sampler must identify the current Alan process"
    )
    try expect(sample.role == .alanApp, "current process sample must use alanApp role")
    try expect(sample.cpuPercent >= 0, "current process CPU must be numeric and nonnegative")
    try expect(
        sample.memoryBytes != nil && sample.memoryBytes! > 0,
        "current process sample must include resident memory"
    )
    try expect(
        sample.threadCount != nil && sample.threadCount! > 0,
        "current process sample must include thread count"
    )
}

private func testKnownTerminalChildAggregateSamplingOmitsCommandIdentity() throws {
    let sample = try expectNonNil(
        AlanPerformanceProcessSampler.aggregateKnownTerminalChildSample(
            [
                AlanPerformanceChildProcessObservation(
                    processID: 101,
                    parentProcessID: 42,
                    cpuPercent: 12.5,
                    memoryBytes: 1_024,
                    threadCount: 2
                ),
                AlanPerformanceChildProcessObservation(
                    processID: 102,
                    parentProcessID: 42,
                    cpuPercent: 25.0,
                    memoryBytes: 2_048,
                    threadCount: 3
                ),
            ],
            timestampMs: 123
        ),
        "known terminal child observations must produce an aggregate sample"
    )

    try expect(sample.timestampMs == 123, "aggregate sample must preserve timestamp")
    try expect(sample.processID == 0, "aggregate child samples must use a sentinel process id")
    try expect(sample.parentProcessID == nil, "aggregate child samples must not expose parent identity")
    try expect(sample.role == .terminalChild, "aggregate sample must use terminalChild role")
    try expect(sample.cpuPercent == 37.5, "aggregate CPU must sum known child observations")
    try expect(sample.memoryBytes == 3_072, "aggregate memory must sum known child observations")
    try expect(sample.threadCount == 5, "aggregate thread count must sum known child observations")

    let unknown = AlanPerformanceProcessSampler.unknownChildPressureSample(
        cpuPercent: 88,
        memoryBytes: nil,
        threadCount: nil,
        timestampMs: 456
    )
    try expect(unknown.role == .unknownChild, "unknown child pressure must be explicit")
    try expect(unknown.processID == 0, "unknown child pressure must use a sentinel process id")
}

private func testDescendantProcessSamplerAggregatesChildPressure() throws {
    let rootPID: Int32 = 500
    let childPID: Int32 = 501
    let grandchildPID: Int32 = 502
    let unrelatedPID: Int32 = 900
    let processes = [
        AlanPerformanceProcessIdentity(processID: childPID, parentProcessID: rootPID),
        AlanPerformanceProcessIdentity(processID: grandchildPID, parentProcessID: childPID),
        AlanPerformanceProcessIdentity(processID: unrelatedPID, parentProcessID: 1),
    ]
    var metricsByPID: [Int32: AlanPerformanceProcessTaskMetrics] = [
        childPID: AlanPerformanceProcessTaskMetrics(
            cpuTimeNanos: 1_000_000_000,
            memoryBytes: 1_024,
            threadCount: 2
        ),
        grandchildPID: AlanPerformanceProcessTaskMetrics(
            cpuTimeNanos: 2_000_000_000,
            memoryBytes: 2_048,
            threadCount: 3
        ),
        unrelatedPID: AlanPerformanceProcessTaskMetrics(
            cpuTimeNanos: 10_000_000_000,
            memoryBytes: 4_096,
            threadCount: 4
        ),
    ]
    let sampler = AlanPerformanceDescendantProcessSampler(
        processProvider: { processes },
        metricsProvider: { metricsByPID[$0] }
    )

    let first = try expectNonNil(
        sampler.sampleDescendants(
            of: rootPID,
            timestampMs: 100,
            uptimeNanos: 1_000_000_000
        ),
        "first descendant sample must establish a child-pressure baseline"
    )
    try expect(first.role == .unknownChild, "app descendant aggregate must use unknownChild role")
    try expect(first.cpuPercent == 0, "first descendant sample must not invent CPU pressure")
    try expect(first.memoryBytes == 3_072, "descendant sample must aggregate child memory")
    try expect(first.threadCount == 5, "descendant sample must aggregate child thread counts")

    metricsByPID[childPID] = AlanPerformanceProcessTaskMetrics(
        cpuTimeNanos: 1_250_000_000,
        memoryBytes: 1_500,
        threadCount: 2
    )
    metricsByPID[grandchildPID] = AlanPerformanceProcessTaskMetrics(
        cpuTimeNanos: 2_500_000_000,
        memoryBytes: 2_500,
        threadCount: 4
    )
    metricsByPID[unrelatedPID] = AlanPerformanceProcessTaskMetrics(
        cpuTimeNanos: 11_000_000_000,
        memoryBytes: 8_000,
        threadCount: 8
    )

    let second = try expectNonNil(
        sampler.sampleDescendants(
            of: rootPID,
            timestampMs: 200,
            uptimeNanos: 2_000_000_000
        ),
        "second descendant sample must produce aggregate child pressure"
    )
    try expect(second.role == .unknownChild, "updated app descendant aggregate must stay unknownChild")
    try expect(second.processID == 0, "descendant aggregate must not expose child PIDs")
    try expect(second.parentProcessID == nil, "descendant aggregate must not expose parent PIDs")
    try expect(second.cpuPercent == 75, "descendant CPU must use cumulative task-time deltas")
    try expect(second.memoryBytes == 4_000, "descendant sample must keep updated aggregate memory")
    try expect(second.threadCount == 6, "descendant sample must keep updated thread counts")
}

private func testBoundedRingBufferAndAutomaticStutterMarker() throws {
    let recorder = AlanPerformanceDiagnosticsRecorder(
        configuration: AlanPerformanceDiagnosticsConfiguration(
            maxEvents: 3,
            slowEventThresholdMs: 50
        )
    )
    recorder.setEnabled(true)

    recorder.record(AlanPerformanceDiagnosticEvent(kind: .runtimeSnapshotPublish, durationMs: 1))
    recorder.record(AlanPerformanceDiagnosticEvent(kind: .terminalMetadataCallback, durationMs: 2))
    recorder.record(AlanPerformanceDiagnosticEvent(kind: .shellRuntimeProjection, durationMs: 75))

    let events = recorder.eventsSnapshot()
    try expect(events.count == 3, "ring buffer must stay bounded")
    try expect(
        events.contains { $0.kind == .automaticStutterMarker },
        "slow events must create automatic stutter markers"
    )
    try expect(
        !events.contains { $0.kind == .runtimeSnapshotPublish },
        "oldest events must be evicted when markers consume bounded capacity"
    )

    let summary = recorder.summarySnapshot()
    try expect(
        summary.totalEventsRecorded == 4,
        "summary must preserve total event count even after ring eviction"
    )
    try expect(summary.stutterMarkerCount == 1, "summary must count stutter markers")
    try expect(
        summary.countsByKind[.shellRuntimeProjection] == 1,
        "summary must group event counts by kind"
    )
}

private func testProcessSamplesStayBounded() throws {
    let recorder = AlanPerformanceDiagnosticsRecorder(
        configuration: AlanPerformanceDiagnosticsConfiguration(
            maxEvents: 8,
            maxProcessSamples: 2
        )
    )
    recorder.setEnabled(true)

    for index in 0..<4 {
        recorder.recordProcessSample(
            AlanPerformanceProcessSample(
                timestampMs: Int64(index),
                processID: Int32(index + 100),
                role: .alanApp,
                cpuPercent: Double(index)
            )
        )
    }

    let samples = recorder.summarySnapshot().processSamples
    try expect(samples.count == 2, "process samples must stay bounded")
    try expect(
        samples.map(\.cpuPercent) == [2, 3],
        "bounded process samples must retain the most recent observations"
    )
}

private func testExportBundleOmitsSensitiveFixtureStrings() throws {
    let root = try makeTemporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }

    let sensitiveValues = [
        "terminal secret output",
        "codex --dangerous-prompt",
        "/Users/morris/Developer/private-repo",
        "OPENAI_API_KEY=sk-secret",
        "refresh-token-secret",
    ]

    let recorder = AlanPerformanceDiagnosticsRecorder(
        configuration: AlanPerformanceDiagnosticsConfiguration(maxEvents: 8)
    )
    recorder.setEnabled(true)
    recorder.record(
        AlanPerformanceDiagnosticEvent(
            kind: .shellRuntimeProjection,
            durationMs: 4,
            paneID: sensitiveValues[0],
            contentID: sensitiveValues[2],
            priority: "hiddenBackground",
            visibility: "hidden",
            thread: "main",
            counts: AlanPerformanceDiagnosticCounts(bytes: 120, lines: 3, events: 1)
        )
    )
    recorder.record(
        AlanPerformanceDiagnosticEvent(
            kind: .runtimeSnapshotPublish,
            durationMs: 12,
            paneID: "pane_2",
            contentID: "terminal_pane_2",
            priority: "foregroundInteractive",
            visibility: "visible",
            thread: "main"
        )
    )
    recorder.recordProcessSample(
        AlanPerformanceProcessSample(
            processID: 42,
            parentProcessID: 1,
            role: .terminalChild,
            cpuPercent: 92.5,
            memoryBytes: 128_000,
            threadCount: 9
        )
    )
    recorder.recordProcessSample(
        AlanPerformanceProcessSampler.unknownChildPressureSample(cpuPercent: 12.5)
    )

    let bundle = try recorder.exportRecentDiagnostics(
        to: root,
        metadata: AlanPerformanceDiagnosticsExportMetadata(
            appVersion: "test-build",
            installChannel: "Alan Dev",
            schemaVersion: AlanPerformanceDiagnosticsSchema.currentVersion,
            samplingIntervalMs: 1000
        )
    )

    let events = try String(
        contentsOf: bundle.appendingPathComponent("events.jsonl"),
        encoding: .utf8
    )
    let summaryData = try Data(contentsOf: bundle.appendingPathComponent("summary.json"))
    let summary = String(data: summaryData, encoding: .utf8) ?? ""
    let exported = events + "\n" + summary

    let eventLines = events.split(separator: "\n", omittingEmptySubsequences: true)
    try expect(eventLines.count == 2, "events.jsonl must contain one compact JSON object per event line")
    try expect(
        eventLines.allSatisfy { line in
            line.first == "{" && line.last == "}" && !line.contains("\n")
        },
        "events.jsonl event records must not be pretty-printed across multiple lines"
    )
    try expect(events.contains("pane_id_hash"), "export must keep redacted pane correlation")
    try expect(events.contains("content_id_hash"), "export must keep redacted content correlation")
    try expect(summary.contains("\"processSamples\""), "summary must include process samples")
    try expect(summary.contains("\"terminalChild\""), "summary must include child CPU pressure")
    try expect(summary.contains("\"unknownChild\""), "summary must include unknown child pressure")

    let summaryObject = try expectNonNil(
        JSONSerialization.jsonObject(with: summaryData) as? [String: Any],
        "summary must be a JSON object"
    )
    let countsByKind = try expectNonNil(
        summaryObject["countsByKind"] as? [String: Any],
        "summary countsByKind must be keyed by event kind name"
    )
    let shellProjectionCount = try expectNonNil(
        countsByKind["shellRuntimeProjection"] as? NSNumber,
        "summary countsByKind must include shell runtime projection count"
    )
    try expect(shellProjectionCount.intValue == 1, "summary countsByKind must preserve event counts")
    let countsByPriority = try expectNonNil(
        summaryObject["countsByPriority"] as? [String: Any],
        "summary countsByPriority must be keyed by render priority"
    )
    try expect(
        (countsByPriority["hiddenBackground"] as? NSNumber)?.intValue == 1,
        "summary countsByPriority must include hidden background events"
    )
    try expect(
        (countsByPriority["foregroundInteractive"] as? NSNumber)?.intValue == 1,
        "summary countsByPriority must include foreground interactive events"
    )
    let countsByVisibility = try expectNonNil(
        summaryObject["countsByVisibility"] as? [String: Any],
        "summary countsByVisibility must be keyed by visibility"
    )
    try expect(
        (countsByVisibility["hidden"] as? NSNumber)?.intValue == 1,
        "summary countsByVisibility must include hidden events"
    )
    try expect(
        (countsByVisibility["visible"] as? NSNumber)?.intValue == 1,
        "summary countsByVisibility must include visible events"
    )
    let durationByKind = try expectNonNil(
        summaryObject["durationByKind"] as? [String: Any],
        "summary durationByKind must include retained-window duration statistics"
    )
    let runtimeDurationStats = try expectNonNil(
        durationByKind["runtimeSnapshotPublish"] as? [String: Any],
        "summary durationByKind must include runtime publication stats"
    )
    try expect(
        (runtimeDurationStats["p95_ms"] as? NSNumber)?.doubleValue == 12,
        "summary durationByKind must include p95 duration"
    )
    try expect(
        (runtimeDurationStats["maximum_ms"] as? NSNumber)?.doubleValue == 12,
        "summary durationByKind must include max duration"
    )

    for sensitive in sensitiveValues {
        try expect(
            !exported.contains(sensitive),
            "diagnostics export must omit sensitive fixture value: \(sensitive)"
        )
    }
    try expect(!exported.contains("commandLine"), "export must not include command-line fields")
    try expect(!exported.contains("cwd"), "export must not include cwd fields")
    try expect(!exported.contains("environment"), "export must not include environment fields")
}

private func expectNonNil<T>(_ value: T?, _ message: String) throws -> T {
    guard let value else {
        throw TestFailure.message(message)
    }
    return value
}

private func makeTemporaryDirectory() throws -> URL {
    let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        .appendingPathComponent("alan-performance-diagnostics-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}
#endif
