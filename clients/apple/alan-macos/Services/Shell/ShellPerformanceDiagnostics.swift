import Foundation
import Darwin

#if os(macOS)
enum AlanPerformanceDiagnosticsSchema {
    static let currentVersion = 1
}

struct AlanPerformanceDiagnosticsConfiguration: Equatable {
    var maxEvents: Int
    var maxProcessSamples: Int
    var slowEventThresholdMs: Double
    var samplingIntervalMs: Int

    init(
        maxEvents: Int = 5_000,
        maxProcessSamples: Int = 600,
        slowEventThresholdMs: Double = 100,
        samplingIntervalMs: Int = 1_000
    ) {
        self.maxEvents = max(1, maxEvents)
        self.maxProcessSamples = max(1, maxProcessSamples)
        self.slowEventThresholdMs = slowEventThresholdMs
        self.samplingIntervalMs = max(250, samplingIntervalMs)
    }
}

enum AlanPerformanceDiagnosticEventKind: String, Codable, CaseIterable, Equatable, Hashable {
    case ghosttyWakeup
    case ghosttyAppTick
    case ghosttySurfaceRefresh
    case terminalSurfaceAttach
    case terminalCatchUpRefresh
    case runtimeSnapshotPublish
    case terminalMetadataCallback
    case terminalScrollbackUpdate
    case terminalRendererUpdate
    case runtimePriorityChange
    case runtimeVisibilityChange
    case shellRuntimeProjection
    case shellPaneStatePublication
    case shellSelectionChange
    case shellFocusChange
    case shellPrioritySynchronization
    case automaticStutterMarker
}

struct AlanPerformanceDiagnosticCounts: Codable, Equatable {
    var bytes: Int?
    var lines: Int?
    var events: Int?

    init(bytes: Int? = nil, lines: Int? = nil, events: Int? = nil) {
        self.bytes = bytes
        self.lines = lines
        self.events = events
    }
}

struct AlanPerformanceDiagnosticEvent: Codable, Equatable {
    let timestampMs: Int64
    let kind: AlanPerformanceDiagnosticEventKind
    let durationMs: Double
    let paneID: String?
    let contentID: String?
    let priority: String?
    let visibility: String?
    let thread: String?
    let counts: AlanPerformanceDiagnosticCounts?

    init(
        timestampMs: Int64 = AlanPerformanceDiagnosticEvent.currentTimestampMs(),
        kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double = 0,
        paneID: String? = nil,
        contentID: String? = nil,
        priority: String? = nil,
        visibility: String? = nil,
        thread: String? = nil,
        counts: AlanPerformanceDiagnosticCounts? = nil
    ) {
        self.timestampMs = timestampMs
        self.kind = kind
        self.durationMs = durationMs
        self.paneID = paneID
        self.contentID = contentID
        self.priority = priority
        self.visibility = visibility
        self.thread = thread
        self.counts = counts
    }

    private static func currentTimestampMs() -> Int64 {
        Int64((Date().timeIntervalSince1970 * 1_000).rounded())
    }
}

enum AlanPerformanceProcessRole: String, Codable, Equatable {
    case alanApp
    case terminalChild
    case unknownChild
}

struct AlanPerformanceProcessSample: Codable, Equatable {
    let timestampMs: Int64
    let processID: Int32
    let parentProcessID: Int32?
    let role: AlanPerformanceProcessRole
    let cpuPercent: Double
    let memoryBytes: UInt64?
    let threadCount: Int?

    init(
        timestampMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded()),
        processID: Int32,
        parentProcessID: Int32? = nil,
        role: AlanPerformanceProcessRole,
        cpuPercent: Double,
        memoryBytes: UInt64? = nil,
        threadCount: Int? = nil
    ) {
        self.timestampMs = timestampMs
        self.processID = processID
        self.parentProcessID = parentProcessID
        self.role = role
        self.cpuPercent = cpuPercent
        self.memoryBytes = memoryBytes
        self.threadCount = threadCount
    }
}

struct AlanPerformanceChildProcessObservation: Equatable {
    let processID: Int32
    let parentProcessID: Int32?
    let cpuPercent: Double
    let memoryBytes: UInt64?
    let threadCount: Int?

    init(
        processID: Int32,
        parentProcessID: Int32? = nil,
        cpuPercent: Double,
        memoryBytes: UInt64? = nil,
        threadCount: Int? = nil
    ) {
        self.processID = processID
        self.parentProcessID = parentProcessID
        self.cpuPercent = max(0, cpuPercent)
        self.memoryBytes = memoryBytes
        self.threadCount = threadCount
    }
}

struct AlanPerformanceProcessIdentity: Equatable {
    let processID: Int32
    let parentProcessID: Int32
}

struct AlanPerformanceProcessTaskMetrics: Equatable {
    let cpuTimeNanos: UInt64
    let memoryBytes: UInt64?
    let threadCount: Int?
}

final class AlanPerformanceDescendantProcessSampler {
    typealias ProcessProvider = () -> [AlanPerformanceProcessIdentity]
    typealias MetricsProvider = (Int32) -> AlanPerformanceProcessTaskMetrics?

    private struct CPUBaseline {
        let cpuTimeNanos: UInt64
        let uptimeNanos: UInt64
    }

    private let processProvider: ProcessProvider
    private let metricsProvider: MetricsProvider
    private var cpuBaselinesByProcessID: [Int32: CPUBaseline] = [:]

    init(
        processProvider: @escaping ProcessProvider = AlanPerformanceSystemProcessProvider
            .allProcessIdentities,
        metricsProvider: @escaping MetricsProvider = AlanPerformanceSystemProcessProvider
            .taskMetrics
    ) {
        self.processProvider = processProvider
        self.metricsProvider = metricsProvider
    }

    func sampleDescendants(
        of rootProcessID: Int32 = ProcessInfo.processInfo.processIdentifier,
        timestampMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded()),
        uptimeNanos: UInt64 = DispatchTime.now().uptimeNanoseconds
    ) -> AlanPerformanceProcessSample? {
        let descendants = Self.descendants(
            of: rootProcessID,
            in: processProvider()
        )
        guard !descendants.isEmpty else {
            cpuBaselinesByProcessID.removeAll()
            return nil
        }

        var observations: [AlanPerformanceChildProcessObservation] = []
        var liveProcessIDs = Set<Int32>()
        for process in descendants {
            liveProcessIDs.insert(process.processID)
            guard let metrics = metricsProvider(process.processID) else { continue }
            observations.append(
                AlanPerformanceChildProcessObservation(
                    processID: process.processID,
                    parentProcessID: process.parentProcessID,
                    cpuPercent: cpuPercent(
                        for: process.processID,
                        metrics: metrics,
                        uptimeNanos: uptimeNanos
                    ),
                    memoryBytes: metrics.memoryBytes,
                    threadCount: metrics.threadCount
                )
            )
        }

        cpuBaselinesByProcessID = cpuBaselinesByProcessID.filter { liveProcessIDs.contains($0.key) }
        guard !observations.isEmpty else { return nil }
        let memoryValues = observations.compactMap(\.memoryBytes)
        let threadValues = observations.compactMap(\.threadCount)
        return AlanPerformanceProcessSampler.unknownChildPressureSample(
            cpuPercent: observations.reduce(0) { $0 + $1.cpuPercent },
            memoryBytes: memoryValues.isEmpty ? nil : memoryValues.reduce(0, +),
            threadCount: threadValues.isEmpty ? nil : threadValues.reduce(0, +),
            timestampMs: timestampMs
        )
    }

    func reset() {
        cpuBaselinesByProcessID.removeAll()
    }

    private func cpuPercent(
        for processID: Int32,
        metrics: AlanPerformanceProcessTaskMetrics,
        uptimeNanos: UInt64
    ) -> Double {
        defer {
            cpuBaselinesByProcessID[processID] = CPUBaseline(
                cpuTimeNanos: metrics.cpuTimeNanos,
                uptimeNanos: uptimeNanos
            )
        }

        guard let previous = cpuBaselinesByProcessID[processID],
              uptimeNanos > previous.uptimeNanos,
              metrics.cpuTimeNanos >= previous.cpuTimeNanos
        else {
            return 0
        }

        let cpuDelta = metrics.cpuTimeNanos - previous.cpuTimeNanos
        let uptimeDelta = uptimeNanos - previous.uptimeNanos
        return Double(cpuDelta) / Double(uptimeDelta) * 100
    }

    private static func descendants(
        of rootProcessID: Int32,
        in processes: [AlanPerformanceProcessIdentity]
    ) -> [AlanPerformanceProcessIdentity] {
        let childrenByParent = Dictionary(grouping: processes, by: \.parentProcessID)
        var descendants: [AlanPerformanceProcessIdentity] = []
        var pending = childrenByParent[rootProcessID] ?? []
        while let process = pending.popLast() {
            descendants.append(process)
            pending.append(contentsOf: childrenByParent[process.processID] ?? [])
        }
        return descendants
    }
}

enum AlanPerformanceProcessSampler {
    private static let aggregateProcessID: Int32 = 0

    static func currentAlanProcessSample(
        timestampMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
    ) -> AlanPerformanceProcessSample {
        let metrics = currentTaskMetrics()
        return AlanPerformanceProcessSample(
            timestampMs: timestampMs,
            processID: ProcessInfo.processInfo.processIdentifier,
            parentProcessID: getppid(),
            role: .alanApp,
            cpuPercent: metrics.cpuPercent,
            memoryBytes: metrics.memoryBytes,
            threadCount: metrics.threadCount
        )
    }

    static func aggregateKnownTerminalChildSample(
        _ observations: [AlanPerformanceChildProcessObservation],
        timestampMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
    ) -> AlanPerformanceProcessSample? {
        guard !observations.isEmpty else { return nil }
        let memoryValues = observations.compactMap(\.memoryBytes)
        let threadValues = observations.compactMap(\.threadCount)
        return AlanPerformanceProcessSample(
            timestampMs: timestampMs,
            processID: aggregateProcessID,
            parentProcessID: nil,
            role: .terminalChild,
            cpuPercent: observations.reduce(0) { $0 + $1.cpuPercent },
            memoryBytes: memoryValues.isEmpty ? nil : memoryValues.reduce(0, +),
            threadCount: threadValues.isEmpty ? nil : threadValues.reduce(0, +)
        )
    }

    static func unknownChildPressureSample(
        cpuPercent: Double,
        memoryBytes: UInt64? = nil,
        threadCount: Int? = nil,
        timestampMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
    ) -> AlanPerformanceProcessSample {
        AlanPerformanceProcessSample(
            timestampMs: timestampMs,
            processID: aggregateProcessID,
            parentProcessID: nil,
            role: .unknownChild,
            cpuPercent: max(0, cpuPercent),
            memoryBytes: memoryBytes,
            threadCount: threadCount
        )
    }

    private static func currentTaskMetrics() -> (
        cpuPercent: Double,
        memoryBytes: UInt64?,
        threadCount: Int?
    ) {
        var memoryBytes: UInt64?
        var taskInfo = mach_task_basic_info()
        var taskInfoCount = mach_msg_type_number_t(
            MemoryLayout<mach_task_basic_info_data_t>.size / MemoryLayout<natural_t>.size
        )
        let taskResult = withUnsafeMutablePointer(to: &taskInfo) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(taskInfoCount)) {
                task_info(
                    mach_task_self_,
                    task_flavor_t(MACH_TASK_BASIC_INFO),
                    $0,
                    &taskInfoCount
                )
            }
        }
        if taskResult == KERN_SUCCESS {
            memoryBytes = UInt64(taskInfo.resident_size)
        }

        var threadList: thread_act_array_t?
        var threadCount = mach_msg_type_number_t(0)
        let threadResult = task_threads(mach_task_self_, &threadList, &threadCount)
        guard threadResult == KERN_SUCCESS,
              let threadList
        else {
            return (0, memoryBytes, nil)
        }
        defer {
            let count = Int(threadCount)
            for index in 0..<count {
                mach_port_deallocate(mach_task_self_, threadList[index])
            }
            let size = vm_size_t(count * MemoryLayout<thread_t>.stride)
            vm_deallocate(mach_task_self_, vm_address_t(UInt(bitPattern: threadList)), size)
        }

        var cpuPercent = 0.0
        for index in 0..<Int(threadCount) {
            var threadInfo = thread_basic_info()
            var threadInfoCount = mach_msg_type_number_t(
                MemoryLayout<thread_basic_info_data_t>.size / MemoryLayout<natural_t>.size
            )
            let infoResult = withUnsafeMutablePointer(to: &threadInfo) { pointer in
                pointer.withMemoryRebound(to: integer_t.self, capacity: Int(threadInfoCount)) {
                    thread_info(
                        threadList[index],
                        thread_flavor_t(THREAD_BASIC_INFO),
                        $0,
                        &threadInfoCount
                    )
                }
            }
            if infoResult == KERN_SUCCESS,
               (threadInfo.flags & TH_FLAGS_IDLE) == 0
            {
                cpuPercent += Double(threadInfo.cpu_usage) / Double(TH_USAGE_SCALE) * 100
            }
        }

        return (max(0, cpuPercent), memoryBytes, Int(threadCount))
    }
}

enum AlanPerformanceSystemProcessProvider {
    static func allProcessIdentities() -> [AlanPerformanceProcessIdentity] {
        let initialBytes = proc_listpids(UInt32(PROC_ALL_PIDS), 0, nil, 0)
        guard initialBytes > 0 else { return [] }

        let capacity = Int(initialBytes) / MemoryLayout<pid_t>.stride + 64
        var pids = Array(repeating: pid_t(0), count: capacity)
        let returnedBytes = pids.withUnsafeMutableBufferPointer { buffer in
            proc_listpids(
                UInt32(PROC_ALL_PIDS),
                0,
                buffer.baseAddress,
                Int32(buffer.count * MemoryLayout<pid_t>.stride)
            )
        }
        guard returnedBytes > 0 else { return [] }

        let count = min(Int(returnedBytes) / MemoryLayout<pid_t>.stride, pids.count)
        return pids.prefix(count).compactMap { pid in
            guard pid > 0 else { return nil }
            var info = proc_bsdinfo()
            let result = proc_pidinfo(
                pid,
                Int32(PROC_PIDTBSDINFO),
                0,
                &info,
                Int32(MemoryLayout<proc_bsdinfo>.size)
            )
            guard result == MemoryLayout<proc_bsdinfo>.size else { return nil }
            return AlanPerformanceProcessIdentity(
                processID: Int32(info.pbi_pid),
                parentProcessID: Int32(info.pbi_ppid)
            )
        }
    }

    static func taskMetrics(for processID: Int32) -> AlanPerformanceProcessTaskMetrics? {
        var info = proc_taskinfo()
        let result = proc_pidinfo(
            pid_t(processID),
            Int32(PROC_PIDTASKINFO),
            0,
            &info,
            Int32(MemoryLayout<proc_taskinfo>.size)
        )
        guard result == MemoryLayout<proc_taskinfo>.size else { return nil }
        return AlanPerformanceProcessTaskMetrics(
            cpuTimeNanos: UInt64(info.pti_total_user) + UInt64(info.pti_total_system),
            memoryBytes: UInt64(info.pti_resident_size),
            threadCount: Int(info.pti_threadnum)
        )
    }
}

struct AlanPerformanceDiagnosticsSummary: Codable, Equatable {
    var totalEventsRecorded: Int
    var stutterMarkerCount: Int
    var countsByKind: [AlanPerformanceDiagnosticEventKind: Int]
    var processSamples: [AlanPerformanceProcessSample]

    static let empty = AlanPerformanceDiagnosticsSummary(
        totalEventsRecorded: 0,
        stutterMarkerCount: 0,
        countsByKind: [:],
        processSamples: []
    )
}

struct AlanPerformanceDiagnosticsExportMetadata: Codable, Equatable {
    let appVersion: String
    let installChannel: String
    let schemaVersion: Int
    let samplingIntervalMs: Int
    let exportedAtMs: Int64

    init(
        appVersion: String,
        installChannel: String,
        schemaVersion: Int,
        samplingIntervalMs: Int,
        exportedAtMs: Int64 = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
    ) {
        self.appVersion = appVersion
        self.installChannel = installChannel
        self.schemaVersion = schemaVersion
        self.samplingIntervalMs = samplingIntervalMs
        self.exportedAtMs = exportedAtMs
    }
}

final class AlanPerformanceDiagnosticsRecorder {
    private let lock = NSLock()
    private let configuration: AlanPerformanceDiagnosticsConfiguration
    private var enabled = false
    private var events: [AlanPerformanceDiagnosticEvent] = []
    private var summary = AlanPerformanceDiagnosticsSummary.empty

    init(configuration: AlanPerformanceDiagnosticsConfiguration = AlanPerformanceDiagnosticsConfiguration()) {
        self.configuration = configuration
    }

    var isEnabled: Bool {
        lock.lock()
        let value = enabled
        lock.unlock()
        return value
    }

    func setEnabled(_ isEnabled: Bool) {
        lock.lock()
        enabled = isEnabled
        if !isEnabled {
            clearLocked()
        }
        lock.unlock()
    }

    func record(_ event: AlanPerformanceDiagnosticEvent) {
        lock.lock()
        guard enabled else {
            lock.unlock()
            return
        }

        appendLocked(event)
        if event.kind != .automaticStutterMarker,
           event.durationMs >= configuration.slowEventThresholdMs
        {
            appendLocked(
                AlanPerformanceDiagnosticEvent(
                    timestampMs: event.timestampMs,
                    kind: .automaticStutterMarker,
                    durationMs: event.durationMs,
                    paneID: event.paneID,
                    contentID: event.contentID,
                    priority: event.priority,
                    visibility: event.visibility,
                    thread: event.thread
                )
            )
        }
        lock.unlock()
    }

    func recordProcessSample(_ sample: AlanPerformanceProcessSample) {
        lock.lock()
        guard enabled else {
            lock.unlock()
            return
        }
        summary.processSamples.append(sample)
        if summary.processSamples.count > configuration.maxProcessSamples {
            summary.processSamples.removeFirst(
                summary.processSamples.count - configuration.maxProcessSamples
            )
        }
        lock.unlock()
    }

    func eventsSnapshot() -> [AlanPerformanceDiagnosticEvent] {
        lock.lock()
        let snapshot = events
        lock.unlock()
        return snapshot
    }

    func summarySnapshot() -> AlanPerformanceDiagnosticsSummary {
        lock.lock()
        let snapshot = summary
        lock.unlock()
        return snapshot
    }

    func exportRecentDiagnostics(
        to directory: URL,
        metadata: AlanPerformanceDiagnosticsExportMetadata
    ) throws -> URL {
        lock.lock()
        let eventSnapshot = events
        let summarySnapshot = summary
        lock.unlock()

        let bundle = directory.appendingPathComponent(
            "alan-performance-diagnostics-\(metadata.exportedAtMs)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)

        let eventEncoder = JSONEncoder()
        eventEncoder.outputFormatting = [.sortedKeys]

        let summaryEncoder = JSONEncoder()
        summaryEncoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        let exportEvents = eventSnapshot.map(AlanPerformanceDiagnosticsExportEvent.init(event:))
        let eventLines = try exportEvents.map { event in
            String(data: try eventEncoder.encode(event), encoding: .utf8) ?? "{}"
        }
        try eventLines
            .joined(separator: "\n")
            .appending(eventLines.isEmpty ? "" : "\n")
            .write(
                to: bundle.appendingPathComponent("events.jsonl"),
                atomically: true,
                encoding: .utf8
            )

        let exportSummary = AlanPerformanceDiagnosticsExportSummary(
            metadata: metadata,
            summary: summarySnapshot,
            events: eventSnapshot,
            retainedEventCount: eventSnapshot.count,
            captureWindow: AlanPerformanceDiagnosticsCaptureWindow(events: eventSnapshot)
        )
        try summaryEncoder
            .encode(exportSummary)
            .write(to: bundle.appendingPathComponent("summary.json"), options: [.atomic])

        return bundle
    }

    private func appendLocked(_ event: AlanPerformanceDiagnosticEvent) {
        events.append(event)
        if events.count > configuration.maxEvents {
            events.removeFirst(events.count - configuration.maxEvents)
        }

        summary.totalEventsRecorded += 1
        summary.countsByKind[event.kind, default: 0] += 1
        if event.kind == .automaticStutterMarker {
            summary.stutterMarkerCount += 1
        }
    }

    private func clearLocked() {
        events.removeAll()
        summary = .empty
    }
}

final class AlanPerformanceDiagnosticsController {
    static let preferenceKey = "alanPerformanceDiagnosticsEnabled"
    static let shared = AlanPerformanceDiagnosticsController()

    private let defaults: UserDefaults
    private let recorder: AlanPerformanceDiagnosticsRecorder
    private let descendantProcessSampler: AlanPerformanceDescendantProcessSampler
    private let samplingIntervalMs: Int
    private let samplingQueue = DispatchQueue(label: "alan.performance-diagnostics.sampler")
    private var samplingTimer: DispatchSourceTimer?

    init(
        defaults: UserDefaults = .standard,
        recorder: AlanPerformanceDiagnosticsRecorder = AlanPerformanceDiagnosticsRecorder(),
        descendantProcessSampler: AlanPerformanceDescendantProcessSampler =
            AlanPerformanceDescendantProcessSampler(),
        samplingIntervalMs: Int = AlanPerformanceDiagnosticsConfiguration().samplingIntervalMs
    ) {
        self.defaults = defaults
        self.recorder = recorder
        self.descendantProcessSampler = descendantProcessSampler
        self.samplingIntervalMs = max(250, samplingIntervalMs)
        recorder.setEnabled(defaults.bool(forKey: Self.preferenceKey))
        if recorder.isEnabled {
            startProcessSampling()
        }
    }

    var isEnabled: Bool {
        recorder.isEnabled
    }

    func setEnabled(_ isEnabled: Bool) {
        defaults.set(isEnabled, forKey: Self.preferenceKey)
        recorder.setEnabled(isEnabled)
        if isEnabled {
            startProcessSampling()
        } else {
            stopProcessSampling()
        }
    }

    func record(
        _ kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double = 0,
        paneID: String? = nil,
        contentID: String? = nil,
        priority: String? = nil,
        visibility: String? = nil,
        thread: String? = nil,
        counts: AlanPerformanceDiagnosticCounts? = nil
    ) {
        guard recorder.isEnabled else { return }
        recorder.record(
            AlanPerformanceDiagnosticEvent(
                kind: kind,
                durationMs: durationMs,
                paneID: paneID,
                contentID: contentID,
                priority: priority,
                visibility: visibility,
                thread: thread,
                counts: counts
            )
        )
    }

    func recordProcessSample(_ sample: AlanPerformanceProcessSample) {
        guard recorder.isEnabled else { return }
        recorder.recordProcessSample(sample)
    }

    func recordKnownTerminalChildProcesses(
        _ observations: [AlanPerformanceChildProcessObservation]
    ) {
        guard recorder.isEnabled else { return }
        guard let sample = AlanPerformanceProcessSampler.aggregateKnownTerminalChildSample(
            observations
        ) else {
            return
        }
        recorder.recordProcessSample(sample)
    }

    func recordUnknownChildPressure(
        cpuPercent: Double,
        memoryBytes: UInt64? = nil,
        threadCount: Int? = nil
    ) {
        guard recorder.isEnabled else { return }
        recorder.recordProcessSample(
            AlanPerformanceProcessSampler.unknownChildPressureSample(
                cpuPercent: cpuPercent,
                memoryBytes: memoryBytes,
                threadCount: threadCount
            )
        )
    }

    func summarySnapshot() -> AlanPerformanceDiagnosticsSummary {
        recorder.summarySnapshot()
    }

    func eventsSnapshot() -> [AlanPerformanceDiagnosticEvent] {
        recorder.eventsSnapshot()
    }

    func exportRecentDiagnostics(
        to directory: URL,
        appVersion: String,
        installChannel: String
    ) throws -> URL {
        try recorder.exportRecentDiagnostics(
            to: directory,
            metadata: AlanPerformanceDiagnosticsExportMetadata(
                appVersion: appVersion,
                installChannel: installChannel,
                schemaVersion: AlanPerformanceDiagnosticsSchema.currentVersion,
                samplingIntervalMs: samplingIntervalMs
            )
        )
    }

    private func startProcessSampling() {
        samplingQueue.async { [weak self] in
            guard let self else { return }
            samplingTimer?.cancel()
            let timer = DispatchSource.makeTimerSource(queue: samplingQueue)
            timer.schedule(
                deadline: .now(),
                repeating: .milliseconds(samplingIntervalMs),
                leeway: .milliseconds(max(50, samplingIntervalMs / 10))
            )
            timer.setEventHandler { [weak self] in
                guard let self,
                      recorder.isEnabled
                else {
                    return
                }
                let timestampMs = Int64((Date().timeIntervalSince1970 * 1_000).rounded())
                recorder.recordProcessSample(
                    AlanPerformanceProcessSampler.currentAlanProcessSample(
                        timestampMs: timestampMs
                    )
                )
                if let childSample = descendantProcessSampler.sampleDescendants(
                    timestampMs: timestampMs
                ) {
                    recorder.recordProcessSample(childSample)
                }
            }
            samplingTimer = timer
            timer.resume()
        }
    }

    private func stopProcessSampling() {
        samplingQueue.async { [weak self] in
            guard let self else { return }
            samplingTimer?.cancel()
            samplingTimer = nil
            descendantProcessSampler.reset()
        }
    }
}

private struct AlanPerformanceDiagnosticsExportEvent: Codable {
    let timestampMs: Int64
    let kind: AlanPerformanceDiagnosticEventKind
    let durationMs: Double
    let paneIDHash: String?
    let contentIDHash: String?
    let priority: String?
    let visibility: String?
    let thread: String?
    let counts: AlanPerformanceDiagnosticCounts?

    enum CodingKeys: String, CodingKey {
        case timestampMs = "ts_ms"
        case kind
        case durationMs = "duration_ms"
        case paneIDHash = "pane_id_hash"
        case contentIDHash = "content_id_hash"
        case priority
        case visibility
        case thread
        case counts
    }

    init(event: AlanPerformanceDiagnosticEvent) {
        timestampMs = event.timestampMs
        kind = event.kind
        durationMs = event.durationMs
        paneIDHash = event.paneID.map(AlanPerformanceDiagnosticsHasher.hash)
        contentIDHash = event.contentID.map(AlanPerformanceDiagnosticsHasher.hash)
        priority = event.priority
        visibility = event.visibility
        thread = event.thread
        counts = event.counts
    }
}

private struct AlanPerformanceDiagnosticsCaptureWindow: Codable, Equatable {
    let firstEventAtMs: Int64?
    let lastEventAtMs: Int64?

    init(events: [AlanPerformanceDiagnosticEvent]) {
        firstEventAtMs = events.first?.timestampMs
        lastEventAtMs = events.last?.timestampMs
    }
}

private struct AlanPerformanceDiagnosticsDurationStats: Codable, Equatable {
    let count: Int
    let minimumMs: Double
    let maximumMs: Double
    let averageMs: Double
    let p50Ms: Double
    let p95Ms: Double

    enum CodingKeys: String, CodingKey {
        case count
        case minimumMs = "minimum_ms"
        case maximumMs = "maximum_ms"
        case averageMs = "average_ms"
        case p50Ms = "p50_ms"
        case p95Ms = "p95_ms"
    }

    init?(durations: [Double]) {
        guard !durations.isEmpty else { return nil }
        let sorted = durations.sorted()
        count = sorted.count
        minimumMs = sorted[0]
        maximumMs = sorted[sorted.count - 1]
        averageMs = sorted.reduce(0, +) / Double(sorted.count)
        p50Ms = Self.percentile(0.50, in: sorted)
        p95Ms = Self.percentile(0.95, in: sorted)
    }

    private static func percentile(_ percentile: Double, in sorted: [Double]) -> Double {
        guard let first = sorted.first else { return 0 }
        guard sorted.count > 1 else { return first }
        let clamped = min(max(percentile, 0), 1)
        let index = Int((clamped * Double(sorted.count - 1)).rounded(.up))
        return sorted[min(index, sorted.count - 1)]
    }
}

private struct AlanPerformanceDiagnosticsExportSummary: Codable {
    let metadata: AlanPerformanceDiagnosticsExportMetadata
    let totalEventsRecorded: Int
    let retainedEventCount: Int
    let stutterMarkerCount: Int
    let countsByKind: [String: Int]
    let countsByPriority: [String: Int]
    let countsByVisibility: [String: Int]
    let durationByKind: [String: AlanPerformanceDiagnosticsDurationStats]
    let processSamples: [AlanPerformanceProcessSample]
    let captureWindow: AlanPerformanceDiagnosticsCaptureWindow

    init(
        metadata: AlanPerformanceDiagnosticsExportMetadata,
        summary: AlanPerformanceDiagnosticsSummary,
        events: [AlanPerformanceDiagnosticEvent],
        retainedEventCount: Int,
        captureWindow: AlanPerformanceDiagnosticsCaptureWindow
    ) {
        self.metadata = metadata
        totalEventsRecorded = summary.totalEventsRecorded
        self.retainedEventCount = retainedEventCount
        stutterMarkerCount = summary.stutterMarkerCount
        countsByKind = Dictionary(
            uniqueKeysWithValues: summary.countsByKind.map { ($0.key.rawValue, $0.value) }
        )
        countsByPriority = Self.countsByStringValue(
            events.compactMap(\.priority)
        )
        countsByVisibility = Self.countsByStringValue(
            events.compactMap(\.visibility)
        )
        durationByKind = Self.durationStatsByKind(events)
        processSamples = summary.processSamples
        self.captureWindow = captureWindow
    }

    private static func countsByStringValue(_ values: [String]) -> [String: Int] {
        values.reduce(into: [:]) { counts, value in
            counts[value, default: 0] += 1
        }
    }

    private static func durationStatsByKind(
        _ events: [AlanPerformanceDiagnosticEvent]
    ) -> [String: AlanPerformanceDiagnosticsDurationStats] {
        var durations: [AlanPerformanceDiagnosticEventKind: [Double]] = [:]
        for event in events {
            durations[event.kind, default: []].append(event.durationMs)
        }
        return durations.reduce(into: [:]) { stats, entry in
            guard let value = AlanPerformanceDiagnosticsDurationStats(durations: entry.value) else {
                return
            }
            stats[entry.key.rawValue] = value
        }
    }
}

private enum AlanPerformanceDiagnosticsHasher {
    static func hash(_ value: String) -> String {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in value.utf8 {
            hash ^= UInt64(byte)
            hash &*= 1_099_511_628_211
        }
        return String(format: "%016llx", hash)
    }
}
#endif
