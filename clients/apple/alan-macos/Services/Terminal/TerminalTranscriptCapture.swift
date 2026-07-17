#if os(macOS)
import Foundation

@MainActor
func buildTerminalTranscriptCapture(
    for handle: AlanTerminalSurfaceHandle,
    now: Date = .now
) -> TerminalTranscriptCaptureResult {
    let hostSnapshot = handle.latestHostRuntimeSnapshot
    let surfaceSnapshot = handle.snapshot
    let metrics = hostSnapshot?.surfaceState.scrollback.metrics
    let range = transcriptCaptureRange(metrics: metrics)
    let liveLines = handle.captureTranscriptText(in: range)
        .map(transcriptLines(from:)) ?? []
    let lines = liveLines.isEmpty ? handle.fallbackTranscriptLines : liveLines
    guard !lines.isEmpty else {
        return .failed(
            TerminalTranscriptCaptureFailure(
                contentID: handle.contentID,
                code: .emptyTranscript,
                message: "The terminal runtime did not expose restorable transcript text."
            )
        )
    }

    let metadata = hostSnapshot?.paneMetadata ?? surfaceSnapshot.metadata
    let dimensions = transcriptDimensions(
        ptyDimensions: handle.terminalDimensions,
        metrics: metrics
    )
    let alternateScreen = hostSnapshot?.surfaceState.terminalMode == .alternateScreen
    let snapshot = TerminalTranscriptSnapshot(
        contentID: handle.contentID,
        cwd: metadata.workingDirectory,
        title: metadata.title,
        dimensions: dimensions,
        viewport: TerminalTranscriptViewport(
            firstVisibleRow: metrics?.firstVisibleRow,
            cursorRow: nil
        ),
        transcriptLines: lines,
        processSummary: TerminalTranscriptProcessSummary(
            processState: metadata.processExited
                ? "exited"
                : metadata.activeTaskState?.rawValue,
            program: metadata.activity?.source.label,
            argvPreview: nil,
            lastCommandExitCode: metadata.lastCommandExitCode
        ),
        capturedAt: now,
        alternateScreen: alternateScreen
    )
    return .captured(snapshot.boundedForManifest())
}

private func transcriptCaptureRange(metrics: AlanTerminalScrollbackMetrics?) -> AlanTerminalBufferRange {
    guard let metrics, metrics.totalRows > 0 else {
        return AlanTerminalBufferRange(
            lowerBound: 0,
            upperBound: TerminalTranscriptSnapshot.defaultMaxRows
        )
    }
    let upperBound = max(metrics.totalRows, metrics.firstVisibleRow + metrics.visibleRows)
    return AlanTerminalBufferRange(
        lowerBound: max(0, upperBound - TerminalTranscriptSnapshot.defaultMaxRows),
        upperBound: upperBound
    )
}

func transcriptLines(from text: String) -> [String] {
    text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
}

private func transcriptDimensions(
    ptyDimensions: AlanTerminalPtyDimensions?,
    metrics: AlanTerminalScrollbackMetrics?
) -> TerminalTranscriptDimensions? {
    let columns = ptyDimensions?.columns ?? 0
    let rows = ptyDimensions?.rows ?? metrics?.visibleRows ?? 0
    guard columns > 0 || rows > 0 else { return nil }
    return TerminalTranscriptDimensions(columns: max(0, columns), rows: max(0, rows))
}

#endif
