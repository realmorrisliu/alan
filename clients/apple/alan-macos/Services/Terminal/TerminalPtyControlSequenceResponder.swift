#if os(macOS)
import Foundation

struct AlanTerminalPtyControlSequenceResponder: Equatable {
    private enum ParserState: Equatable {
        case normal
        case escape
        case csi
        case osc
        case oscEscape
    }

    private static let escapeByte: UInt8 = 0x1B
    private static let bellByte: UInt8 = 0x07
    private static let csiByte: UInt8 = 0x9B
    private static let oscByte: UInt8 = 0x9D
    private static let leftBracketByte: UInt8 = 0x5B
    private static let rightBracketByte: UInt8 = 0x5D
    private static let backslashByte: UInt8 = 0x5C
    private static let zeroByte: UInt8 = 0x30
    private static let maxBufferedControlSequenceBytes = 512
    private static let primaryDeviceAttributesResponse = Array("\u{1B}[?62;22c".utf8)
    private static let cursorPositionReportResponse = Array("\u{1B}[1;1R".utf8)
    private static let backgroundColorResponse = Array("\u{1B}]11;rgb:0a0a/0c0c/1010\u{1B}\\".utf8)

    private var state: ParserState = .normal
    private var pendingControlSequence: [UInt8] = []
    private var suppressedPrimaryDeviceAttributesResponses: Int

    init(suppressedPrimaryDeviceAttributesResponses: Int = 0) {
        self.suppressedPrimaryDeviceAttributesResponses = max(
            0,
            suppressedPrimaryDeviceAttributesResponses
        )
    }

    static var primaryDeviceAttributesResponseData: Data {
        Data(primaryDeviceAttributesResponse)
    }

    mutating func suppressNextPrimaryDeviceAttributesResponse() {
        suppressedPrimaryDeviceAttributesResponses += 1
    }

    mutating func process(_ data: Data) -> AlanTerminalPtyControlSequenceResponse {
        var rendererOutput: [UInt8] = []
        var ptyResponse: [UInt8] = []

        for byte in data {
            switch state {
            case .normal:
                if byte == Self.escapeByte {
                    pendingControlSequence = [byte]
                    state = .escape
                } else if byte == Self.csiByte {
                    pendingControlSequence = [byte]
                    state = .csi
                } else if byte == Self.oscByte {
                    pendingControlSequence = [byte]
                    state = .osc
                } else {
                    rendererOutput.append(byte)
                }

            case .escape:
                if byte == Self.leftBracketByte {
                    pendingControlSequence.append(byte)
                    state = .csi
                } else if byte == Self.rightBracketByte {
                    pendingControlSequence.append(byte)
                    state = .osc
                } else {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    rendererOutput.append(byte)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .csi:
                pendingControlSequence.append(byte)
                if Self.isCSIFinalByte(byte) {
                    if Self.isPrimaryDeviceAttributesQuery(pendingControlSequence) {
                        if suppressedPrimaryDeviceAttributesResponses > 0 {
                            suppressedPrimaryDeviceAttributesResponses -= 1
                        } else {
                            ptyResponse.append(contentsOf: Self.primaryDeviceAttributesResponse)
                        }
                    } else if Self.isCursorPositionReportQuery(pendingControlSequence) {
                        ptyResponse.append(contentsOf: Self.cursorPositionReportResponse)
                    } else {
                        rendererOutput.append(contentsOf: pendingControlSequence)
                    }
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .osc:
                pendingControlSequence.append(byte)
                if byte == Self.bellByte {
                    Self.completeOSCSequence(
                        pendingControlSequence,
                        rendererOutput: &rendererOutput,
                        ptyResponse: &ptyResponse
                    )
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if byte == Self.escapeByte {
                    state = .oscEscape
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                }

            case .oscEscape:
                pendingControlSequence.append(byte)
                if byte == Self.backslashByte {
                    Self.completeOSCSequence(
                        pendingControlSequence,
                        rendererOutput: &rendererOutput,
                        ptyResponse: &ptyResponse
                    )
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else if pendingControlSequence.count > Self.maxBufferedControlSequenceBytes {
                    rendererOutput.append(contentsOf: pendingControlSequence)
                    pendingControlSequence.removeAll(keepingCapacity: true)
                    state = .normal
                } else {
                    state = .osc
                }
            }
        }

        return AlanTerminalPtyControlSequenceResponse(
            rendererOutput: Data(rendererOutput),
            ptyResponse: Data(ptyResponse)
        )
    }

    private static func isCSIFinalByte(_ byte: UInt8) -> Bool {
        (0x40...0x7E).contains(byte)
    }

    private static func isPrimaryDeviceAttributesQuery(_ bytes: [UInt8]) -> Bool {
        guard bytes.last == UInt8(ascii: "c") else { return false }

        let parameterStartIndex: Int
        if bytes.first == escapeByte {
            guard bytes.count >= 3, bytes[1] == leftBracketByte else { return false }
            parameterStartIndex = 2
        } else if bytes.first == csiByte {
            guard bytes.count >= 2 else { return false }
            parameterStartIndex = 1
        } else {
            return false
        }

        let parameters = bytes[parameterStartIndex..<(bytes.count - 1)]
        return parameters.isEmpty || (parameters.count == 1 && parameters.first == zeroByte)
    }

    private static func isCursorPositionReportQuery(_ bytes: [UInt8]) -> Bool {
        guard bytes.last == UInt8(ascii: "n") else { return false }

        let parameterStartIndex: Int
        if bytes.first == escapeByte {
            guard bytes.count >= 4, bytes[1] == leftBracketByte else { return false }
            parameterStartIndex = 2
        } else if bytes.first == csiByte {
            guard bytes.count >= 3 else { return false }
            parameterStartIndex = 1
        } else {
            return false
        }

        let parameters = bytes[parameterStartIndex..<(bytes.count - 1)]
        return parameters.count == 1 && parameters.first == UInt8(ascii: "6")
    }

    private static func completeOSCSequence(
        _ bytes: [UInt8],
        rendererOutput: inout [UInt8],
        ptyResponse: inout [UInt8]
    ) {
        if isBackgroundColorQuery(bytes) {
            ptyResponse.append(contentsOf: backgroundColorResponse)
        } else {
            rendererOutput.append(contentsOf: bytes)
        }
    }

    private static func isBackgroundColorQuery(_ bytes: [UInt8]) -> Bool {
        let payloadRange: Range<Int>
        if bytes.first == escapeByte {
            guard bytes.count >= 6, bytes[1] == rightBracketByte else { return false }
            if bytes.last == bellByte {
                payloadRange = 2..<(bytes.count - 1)
            } else if bytes.count >= 7,
                bytes[bytes.count - 2] == escapeByte,
                bytes.last == backslashByte
            {
                payloadRange = 2..<(bytes.count - 2)
            } else {
                return false
            }
        } else if bytes.first == oscByte {
            guard bytes.count >= 5, bytes.last == bellByte else { return false }
            payloadRange = 1..<(bytes.count - 1)
        } else {
            return false
        }

        let payload = String(decoding: bytes[payloadRange], as: UTF8.self)
        return payload == "11;?"
    }
}

#endif
