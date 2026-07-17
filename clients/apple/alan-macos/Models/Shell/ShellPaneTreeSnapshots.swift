import Foundation

struct ShellPaneSlot: Identifiable, Codable, Equatable {
    let paneSlotID: String
    let tabID: String
    let spaceID: String
    let contentID: String
    let attention: ShellAttentionState

    var id: String { paneSlotID }

    private enum CodingKeys: String, CodingKey {
        case paneSlotID = "pane_slot_id"
        case tabID = "tab_id"
        case spaceID = "space_id"
        case contentID = "content_id"
        case attention
    }
}

struct ShellPaneTreeNode: Identifiable, Codable, Equatable {
    static let minimumSplitRatio = 0.15
    static let maximumSplitRatio = 0.85

    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneID: String?
    let children: [ShellPaneTreeNode]?

    var id: String { nodeID }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneID = "pane_id"
        case children
    }

    init(
        nodeID: String,
        kind: ShellPaneTreeKind,
        direction: ShellSplitDirection?,
        ratio: Double? = nil,
        paneID: String?,
        children: [ShellPaneTreeNode]?
    ) {
        self.nodeID = nodeID
        self.kind = kind
        self.direction = direction
        self.ratio = kind == .split
            ? Self.clampedSplitRatio(ratio ?? 0.5)
            : nil
        self.paneID = paneID
        self.children = children
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(ShellPaneTreeKind.self, forKey: .kind)
        let decodedRatio = kind == .split ? try container.decode(Double.self, forKey: .ratio) : nil

        self.init(
            nodeID: try container.decode(String.self, forKey: .nodeID),
            kind: kind,
            direction: try container.decodeIfPresent(ShellSplitDirection.self, forKey: .direction),
            ratio: decodedRatio,
            paneID: try container.decodeIfPresent(String.self, forKey: .paneID),
            children: try container.decodeIfPresent([ShellPaneTreeNode].self, forKey: .children)
        )
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(nodeID, forKey: .nodeID)
        try container.encode(kind, forKey: .kind)
        try container.encodeIfPresent(direction, forKey: .direction)
        if kind == .split {
            try container.encode(ratio ?? 0.5, forKey: .ratio)
        }
        try container.encodeIfPresent(paneID, forKey: .paneID)
        try container.encodeIfPresent(children, forKey: .children)
    }

    static func clampedSplitRatio(_ ratio: Double) -> Double {
        guard ratio.isFinite else { return 0.5 }
        return min(max(ratio, minimumSplitRatio), maximumSplitRatio)
    }

    var splitRatio: Double {
        Self.clampedSplitRatio(ratio ?? 0.5)
    }
}

extension ShellPaneTreeNode {
    var splitNodes: [ShellPaneTreeNode] {
        switch kind {
        case .pane:
            return []
        case .split:
            return [self] + (children ?? []).flatMap(\.splitNodes)
        }
    }

    var splitRatiosByNodeID: [String: Double] {
        Dictionary(uniqueKeysWithValues: splitNodes.map { ($0.nodeID, $0.splitRatio) })
    }

    func splitNodeIDsWithChangedRatios(comparedTo previous: ShellPaneTreeNode) -> [String] {
        let previousRatios = previous.splitRatiosByNodeID
        return splitRatiosByNodeID.keys
            .filter { nodeID in
                guard let previousRatio = previousRatios[nodeID],
                      let currentRatio = splitRatiosByNodeID[nodeID]
                else {
                    return false
                }
                return previousRatio != currentRatio
            }
            .sorted()
    }

    var nodeIDs: [String] {
        [nodeID] + (children ?? []).flatMap(\.nodeIDs)
    }

    var paneIDs: [String] {
        switch kind {
        case .pane:
            return paneID.map { [$0] } ?? []
        case .split:
            return (children ?? []).flatMap(\.paneIDs)
        }
    }

    func contains(paneID targetPaneID: String) -> Bool {
        switch kind {
        case .pane:
            return paneID == targetPaneID
        case .split:
            return (children ?? []).contains { $0.contains(paneID: targetPaneID) }
        }
    }

    func contains(nodeID targetNodeID: String) -> Bool {
        if nodeID == targetNodeID { return true }
        return (children ?? []).contains { $0.contains(nodeID: targetNodeID) }
    }

    func node(nodeID targetNodeID: String) -> ShellPaneTreeNode? {
        if nodeID == targetNodeID { return self }
        return (children ?? []).lazy.compactMap { $0.node(nodeID: targetNodeID) }.first
    }

    func leafNode(containingPaneID targetPaneID: String) -> ShellPaneTreeNode? {
        switch kind {
        case .pane:
            return paneID == targetPaneID ? self : nil
        case .split:
            return (children ?? []).lazy.compactMap {
                $0.leafNode(containingPaneID: targetPaneID)
            }.first
        }
    }

    func adjacentPaneID(
        from targetPaneID: String,
        direction: ShellSpatialFocusDirection
    ) -> String? {
        let frames = leafFrames(in: .unit)
        guard let targetFrame = frames.first(where: { $0.paneID == targetPaneID }) else {
            return nil
        }

        return frames
            .filter { $0.paneID != targetPaneID && targetFrame.isAdjacentCandidate($0, direction: direction) }
            .min { lhs, rhs in
                targetFrame.sortsBefore(lhs, rhs, direction: direction)
            }?
            .paneID
    }

    private struct PaneFrame {
        static let unit = PaneFrame(
            paneID: "",
            minX: 0,
            maxX: 1,
            minY: 0,
            maxY: 1
        )

        let paneID: String
        let minX: Double
        let maxX: Double
        let minY: Double
        let maxY: Double

        var width: Double { max(maxX - minX, 0) }
        var height: Double { max(maxY - minY, 0) }
        var midX: Double { (minX + maxX) / 2 }
        var midY: Double { (minY + maxY) / 2 }

        func replacingPaneID(_ paneID: String) -> PaneFrame {
            PaneFrame(
                paneID: paneID,
                minX: minX,
                maxX: maxX,
                minY: minY,
                maxY: maxY
            )
        }

        func isAdjacentCandidate(
            _ candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Bool {
            let epsilon = 0.000_001
            guard perpendicularOverlap(with: candidate, direction: direction) > epsilon else {
                return false
            }

            switch direction {
            case .left:
                return candidate.maxX <= minX + epsilon
            case .right:
                return candidate.minX >= maxX - epsilon
            case .up:
                return candidate.maxY <= minY + epsilon
            case .down:
                return candidate.minY >= maxY - epsilon
            }
        }

        func sortsBefore(
            _ lhs: PaneFrame,
            _ rhs: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Bool {
            let epsilon = 0.000_001
            let lhsDistance = primaryDistance(to: lhs, direction: direction)
            let rhsDistance = primaryDistance(to: rhs, direction: direction)
            if abs(lhsDistance - rhsDistance) > epsilon {
                return lhsDistance < rhsDistance
            }

            let lhsOverlap = perpendicularOverlap(with: lhs, direction: direction)
            let rhsOverlap = perpendicularOverlap(with: rhs, direction: direction)
            if abs(lhsOverlap - rhsOverlap) > epsilon {
                return lhsOverlap > rhsOverlap
            }

            let lhsCenterDistance = perpendicularCenterDistance(to: lhs, direction: direction)
            let rhsCenterDistance = perpendicularCenterDistance(to: rhs, direction: direction)
            if abs(lhsCenterDistance - rhsCenterDistance) > epsilon {
                return lhsCenterDistance < rhsCenterDistance
            }

            return lhs.paneID < rhs.paneID
        }

        private func primaryDistance(
            to candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left:
                return max(minX - candidate.maxX, 0)
            case .right:
                return max(candidate.minX - maxX, 0)
            case .up:
                return max(minY - candidate.maxY, 0)
            case .down:
                return max(candidate.minY - maxY, 0)
            }
        }

        private func perpendicularOverlap(
            with candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left, .right:
                return max(0, min(maxY, candidate.maxY) - max(minY, candidate.minY))
            case .up, .down:
                return max(0, min(maxX, candidate.maxX) - max(minX, candidate.minX))
            }
        }

        private func perpendicularCenterDistance(
            to candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left, .right:
                return abs(midY - candidate.midY)
            case .up, .down:
                return abs(midX - candidate.midX)
            }
        }
    }

    private func leafFrames(in frame: PaneFrame) -> [PaneFrame] {
        switch kind {
        case .pane:
            guard let paneID else { return [] }
            return [frame.replacingPaneID(paneID)]
        case .split:
            let childNodes = children ?? []
            guard !childNodes.isEmpty else { return [] }

            if childNodes.count == 2 {
                let ratio = splitRatio
                switch direction ?? .horizontal {
                case .vertical:
                    let splitX = frame.minX + frame.width * ratio
                    return childNodes[0].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: splitX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    ) + childNodes[1].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: splitX,
                            maxX: frame.maxX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    )
                case .horizontal:
                    let splitY = frame.minY + frame.height * ratio
                    return childNodes[0].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: frame.minY,
                            maxY: splitY
                        )
                    ) + childNodes[1].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: splitY,
                            maxY: frame.maxY
                        )
                    )
                }
            }

            let childCount = Double(childNodes.count)
            return childNodes.enumerated().flatMap { index, child in
                let start = Double(index) / childCount
                let end = Double(index + 1) / childCount
                switch direction ?? .horizontal {
                case .vertical:
                    let minX = frame.minX + frame.width * start
                    let maxX = frame.minX + frame.width * end
                    return child.leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: minX,
                            maxX: maxX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    )
                case .horizontal:
                    let minY = frame.minY + frame.height * start
                    let maxY = frame.minY + frame.height * end
                    return child.leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: minY,
                            maxY: maxY
                        )
                    )
                }
            }
        }
    }
}

struct ShellPaneSlotTreeNode: Identifiable, Codable, Equatable {
    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneSlotID: String?
    let children: [ShellPaneSlotTreeNode]?

    var id: String { nodeID }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneSlotID = "pane_slot_id"
        case children
    }

    init(
        nodeID: String,
        kind: ShellPaneTreeKind,
        direction: ShellSplitDirection?,
        ratio: Double? = nil,
        paneSlotID: String?,
        children: [ShellPaneSlotTreeNode]?
    ) {
        self.nodeID = nodeID
        self.kind = kind
        self.direction = direction
        self.ratio = kind == .split
            ? ShellPaneTreeNode.clampedSplitRatio(ratio ?? 0.5)
            : nil
        self.paneSlotID = paneSlotID
        self.children = children
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(ShellPaneTreeKind.self, forKey: .kind)
        let decodedRatio = kind == .split ? try container.decode(Double.self, forKey: .ratio) : nil

        self.init(
            nodeID: try container.decode(String.self, forKey: .nodeID),
            kind: kind,
            direction: try container.decodeIfPresent(ShellSplitDirection.self, forKey: .direction),
            ratio: decodedRatio,
            paneSlotID: try container.decodeIfPresent(String.self, forKey: .paneSlotID),
            children: try container.decodeIfPresent([ShellPaneSlotTreeNode].self, forKey: .children)
        )
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(nodeID, forKey: .nodeID)
        try container.encode(kind, forKey: .kind)
        try container.encodeIfPresent(direction, forKey: .direction)
        if kind == .split {
            try container.encode(ratio ?? 0.5, forKey: .ratio)
        }
        try container.encodeIfPresent(paneSlotID, forKey: .paneSlotID)
        try container.encodeIfPresent(children, forKey: .children)
    }

    static func migrating(
        paneTree: ShellPaneTreeNode,
        paneIDToSlotID: (String) -> String = { $0 }
    ) -> ShellPaneSlotTreeNode {
        ShellPaneSlotTreeNode(
            nodeID: paneTree.nodeID,
            kind: paneTree.kind,
            direction: paneTree.direction,
            ratio: paneTree.ratio,
            paneSlotID: paneTree.paneID.map(paneIDToSlotID),
            children: paneTree.children?.map {
                ShellPaneSlotTreeNode.migrating(paneTree: $0, paneIDToSlotID: paneIDToSlotID)
            }
        )
    }

    var paneSlotIDs: [String] {
        switch kind {
        case .pane:
            return paneSlotID.map { [$0] } ?? []
        case .split:
            return (children ?? []).flatMap(\.paneSlotIDs)
        }
    }

    func restoringPaneTree() -> ShellPaneTreeNode {
        ShellPaneTreeNode(
            nodeID: nodeID,
            kind: kind,
            direction: direction,
            ratio: ratio,
            paneID: paneSlotID,
            children: children?.map { $0.restoringPaneTree() }
        )
    }
}
