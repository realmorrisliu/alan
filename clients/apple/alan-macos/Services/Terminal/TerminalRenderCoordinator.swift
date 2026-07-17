import Foundation

#if os(macOS)
enum TerminalRuntimeRenderPriority: Int, Codable, Equatable, CaseIterable, Comparable {
    case hiddenBackground = 0
    case visibleBackground = 1
    case foregroundInteractive = 2

    static func < (
        lhs: TerminalRuntimeRenderPriority,
        rhs: TerminalRuntimeRenderPriority
    ) -> Bool {
        lhs.rawValue < rhs.rawValue
    }

    var isVisible: Bool {
        self != .hiddenBackground
    }

    var isForegroundInteractive: Bool {
        self == .foregroundInteractive
    }

    var diagnosticsValue: String {
        switch self {
        case .foregroundInteractive:
            return "foregroundInteractive"
        case .visibleBackground:
            return "visibleBackground"
        case .hiddenBackground:
            return "hiddenBackground"
        }
    }

    var diagnosticsVisibility: String {
        isVisible ? "visible" : "hidden"
    }
}

enum TerminalRenderRefreshReason: String, Equatable {
    case automatic
    case catchUp = "catch_up"
}

struct TerminalRenderCoordinatorMetrics: Codable, Equatable {
    var wakeupRequests = 0
    var appTicks = 0
    var surfaceRefreshes = 0
    var coalescedSurfaceRefreshes = 0
    var catchUpRefreshes = 0
    var cancelledDrains = 0
    var drainBatches = 0
    var lastDrainBatchSize = 0
    var maxDrainBatchSize = 0
    var lastDrainLatencyMs = 0.0
    var maxDrainLatencyMs = 0.0
    var foregroundInteractiveDrains = 0
    var visibleBackgroundDrains = 0
    var hiddenBackgroundDrains = 0

    mutating func recordDrain(priority: TerminalRuntimeRenderPriority) {
        switch priority {
        case .foregroundInteractive:
            foregroundInteractiveDrains += 1
        case .visibleBackground:
            visibleBackgroundDrains += 1
        case .hiddenBackground:
            hiddenBackgroundDrains += 1
        }
    }

    mutating func recordDrainBatch(size: Int, latencyMs: Double) {
        drainBatches += 1
        lastDrainBatchSize = size
        maxDrainBatchSize = max(maxDrainBatchSize, size)
        lastDrainLatencyMs = latencyMs
        maxDrainLatencyMs = max(maxDrainLatencyMs, latencyMs)
    }
}

protocol TerminalRenderCoordinatedHost: AnyObject {
    var terminalRenderPriority: TerminalRuntimeRenderPriority { get }
    var isRenderCoordinatorTargetAlive: Bool { get }

    func renderCoordinatorDrainAppTick()
    func renderCoordinatorRefreshSurface(reason: TerminalRenderRefreshReason)
}

final class TerminalRenderCoordinator {
    private struct PendingWakeup {
        weak var host: TerminalRenderCoordinatedHost?
        let sequence: Int
        let enqueuedAt: DispatchTime
        var requiresSurfaceRefresh: Bool
        var reason: TerminalRenderRefreshReason
    }

    private let lock = NSLock()
    private let automaticallyDrains: Bool
    private let diagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?
    private var pendingWakeupsByHost: [ObjectIdentifier: PendingWakeup] = [:]
    private var drainScheduled = false
    private var nextSequence = 0

    private var metrics = TerminalRenderCoordinatorMetrics()

    init(
        automaticallyDrains: Bool = true,
        diagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil
    ) {
        self.automaticallyDrains = automaticallyDrains
        self.diagnosticsRecorder = diagnosticsRecorder
    }

    func requestWakeup(
        from host: TerminalRenderCoordinatedHost,
        requiresSurfaceRefresh: Bool = true
    ) {
        enqueueWakeup(
            from: host,
            reason: .automatic,
            requiresSurfaceRefresh: requiresSurfaceRefresh
        )
    }

    func requestCatchUp(from host: TerminalRenderCoordinatedHost) {
        enqueueWakeup(
            from: host,
            reason: .catchUp,
            requiresSurfaceRefresh: true
        )
    }

    func drainPending() {
        let pendingWakeups = takePendingWakeups()
        guard !pendingWakeups.isEmpty else { return }

        let drainStartedAt = DispatchTime.now()
        let maxDrainLatencyMs = pendingWakeups
            .map { latencyMs(from: $0.enqueuedAt, to: drainStartedAt) }
            .max() ?? 0
        updateMetrics { metrics in
            metrics.recordDrainBatch(size: pendingWakeups.count, latencyMs: maxDrainLatencyMs)
        }

        for pending in pendingWakeups {
            guard let host = pending.host,
                  host.isRenderCoordinatorTargetAlive
            else {
                updateMetrics { metrics in
                    metrics.cancelledDrains += 1
                }
                continue
            }

            let priority = host.terminalRenderPriority
            updateMetrics { metrics in
                metrics.appTicks += 1
                metrics.recordDrain(priority: priority)
            }
            let tickStartedAt = performanceDiagnosticsStartTime()
            host.renderCoordinatorDrainAppTick()
            if let tickStartedAt {
                recordDiagnostics(
                    kind: .ghosttyAppTick,
                    durationMs: latencyMs(from: tickStartedAt, to: DispatchTime.now()),
                    priority: priority
                )
            }

            guard pending.requiresSurfaceRefresh else { continue }
            let shouldRefresh = priority.isVisible || pending.reason == .catchUp
            guard shouldRefresh else {
                updateMetrics { metrics in
                    metrics.coalescedSurfaceRefreshes += 1
                }
                continue
            }

            updateMetrics { metrics in
                metrics.surfaceRefreshes += 1
                if pending.reason == .catchUp {
                    metrics.catchUpRefreshes += 1
                }
            }
            let refreshStartedAt = performanceDiagnosticsStartTime()
            host.renderCoordinatorRefreshSurface(reason: pending.reason)
            if let refreshStartedAt {
                recordDiagnostics(
                    kind: pending.reason == .catchUp
                        ? .terminalCatchUpRefresh
                        : .ghosttySurfaceRefresh,
                    durationMs: latencyMs(from: refreshStartedAt, to: DispatchTime.now()),
                    priority: priority
                )
            }
        }
    }

    func metricsSnapshot() -> TerminalRenderCoordinatorMetrics {
        lock.lock()
        let snapshot = metrics
        lock.unlock()
        return snapshot
    }

    private func enqueueWakeup(
        from host: TerminalRenderCoordinatedHost,
        reason: TerminalRenderRefreshReason,
        requiresSurfaceRefresh: Bool
    ) {
        let shouldScheduleDrain: Bool
        lock.lock()
        metrics.wakeupRequests += 1
        let hostID = ObjectIdentifier(host)
        if var pending = pendingWakeupsByHost[hostID] {
            pending.requiresSurfaceRefresh = pending.requiresSurfaceRefresh || requiresSurfaceRefresh
            if reason == .catchUp {
                pending.reason = .catchUp
            }
            pendingWakeupsByHost[hostID] = pending
        } else {
            pendingWakeupsByHost[hostID] = PendingWakeup(
                host: host,
                sequence: nextSequence,
                enqueuedAt: DispatchTime.now(),
                requiresSurfaceRefresh: requiresSurfaceRefresh,
                reason: reason
            )
            nextSequence += 1
        }
        shouldScheduleDrain = automaticallyDrains && !drainScheduled
        if shouldScheduleDrain {
            drainScheduled = true
        }
        lock.unlock()

        recordDiagnostics(
            kind: .ghosttyWakeup,
            durationMs: 0,
            priority: host.terminalRenderPriority
        )

        guard shouldScheduleDrain else { return }
        DispatchQueue.main.async { [weak self] in
            self?.drainPending()
        }
    }

    private func takePendingWakeups() -> [PendingWakeup] {
        lock.lock()
        let pending = pendingWakeupsByHost.values.sorted { lhs, rhs in
            let lhsPriority = lhs.host?.terminalRenderPriority ?? .hiddenBackground
            let rhsPriority = rhs.host?.terminalRenderPriority ?? .hiddenBackground
            if lhsPriority != rhsPriority {
                return lhsPriority > rhsPriority
            }
            return lhs.sequence < rhs.sequence
        }
        pendingWakeupsByHost.removeAll()
        drainScheduled = false
        lock.unlock()
        return pending
    }

    private func latencyMs(from start: DispatchTime, to end: DispatchTime) -> Double {
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    private func performanceDiagnosticsStartTime() -> DispatchTime? {
        if let diagnosticsRecorder {
            return diagnosticsRecorder.isEnabled ? DispatchTime.now() : nil
        }
        return AlanPerformanceDiagnosticsController.shared.isEnabled ? DispatchTime.now() : nil
    }

    private func updateMetrics(_ update: (inout TerminalRenderCoordinatorMetrics) -> Void) {
        lock.lock()
        update(&metrics)
        lock.unlock()
    }

    private func recordDiagnostics(
        kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double,
        priority: TerminalRuntimeRenderPriority
    ) {
        if let diagnosticsRecorder {
            guard diagnosticsRecorder.isEnabled else { return }
        } else {
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else { return }
        }
        let event = AlanPerformanceDiagnosticEvent(
            kind: kind,
            durationMs: durationMs,
            priority: priority.diagnosticsValue,
            visibility: priority.diagnosticsVisibility,
            thread: Thread.isMainThread ? "main" : "background"
        )
        if let diagnosticsRecorder {
            diagnosticsRecorder.record(event)
        } else {
            AlanPerformanceDiagnosticsController.shared.record(
                event.kind,
                durationMs: event.durationMs,
                paneID: event.paneID,
                contentID: event.contentID,
                priority: event.priority,
                visibility: event.visibility,
                thread: event.thread,
                counts: event.counts
            )
        }
    }
}

func terminalRuntimeRenderPriority(
    paneID: String,
    paneSpaceID: String,
    paneTabID: String,
    selectedSpaceID: String?,
    selectedTabID: String?,
    focusedPaneID: String?,
    visiblePaneIDs: Set<String>,
    windowIsVisible: Bool
) -> TerminalRuntimeRenderPriority {
    guard windowIsVisible,
          paneSpaceID == selectedSpaceID,
          paneTabID == selectedTabID,
          visiblePaneIDs.contains(paneID)
    else {
        return .hiddenBackground
    }

    guard paneID == focusedPaneID else {
        return .visibleBackground
    }
    return .foregroundInteractive
}

#endif
