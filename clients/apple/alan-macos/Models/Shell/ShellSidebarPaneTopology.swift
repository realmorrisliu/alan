import Foundation

struct ShellTabPaneSummary: Equatable {
    let topology: ShellSidebarPaneTopology
    let focusedPaneID: String?

    init(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: [String],
        focusedPaneID: String?
    ) {
        topology = ShellSidebarPaneTopology.classify(
            paneTree: paneTree,
            visiblePaneIDs: visiblePaneIDs
        )
        self.focusedPaneID = visiblePaneIDs.contains(focusedPaneID ?? "") ? focusedPaneID : nil
    }

    var paneIDs: [String] {
        topology.paneIDs
    }

    var paneCount: Int {
        topology.paneCount
    }

    var isComplex: Bool {
        topology.isComplex
    }

    var accessibilityTopologyLabel: String {
        topology.userFacingDescription
    }

    func nextPaneID(after currentPaneID: String?) -> String? {
        guard !paneIDs.isEmpty else { return nil }
        guard let currentPaneID,
              let currentIndex = paneIDs.firstIndex(of: currentPaneID)
        else {
            return paneIDs.first
        }

        return paneIDs[(currentIndex + 1) % paneIDs.count]
    }
}
struct ShellSidebarPaneTopology: Equatable {
    let kind: ShellSidebarPaneTopologyKind
    let paneIDs: [String]

    var paneCount: Int {
        paneIDs.count
    }

    var isComplex: Bool {
        if case .complex = kind {
            return true
        }
        return false
    }

    var userFacingDescription: String {
        switch kind {
        case .single:
            return "Single pane"
        case .columns(let count):
            return "\(count)-column split"
        case .rows(let count):
            return "\(count)-row split"
        case .mainLeftWithRightStack:
            return "Main pane left with right stack"
        case .mainRightWithLeftStack:
            return "Main pane right with left stack"
        case .mainTopWithBottomSplit:
            return "Main pane top with bottom split"
        case .mainBottomWithTopSplit:
            return "Main pane bottom with top split"
        case .grid2x2:
            return "2 by 2 grid split"
        case .complex(let count):
            return "Complex split, \(count) panes"
        }
    }

    static func classify(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: [String]
    ) -> ShellSidebarPaneTopology {
        let orderedVisiblePaneIDs = paneTree.paneIDs.filter { visiblePaneIDs.contains($0) }
        guard !orderedVisiblePaneIDs.isEmpty,
              let normalizedTree = ShellSidebarPaneTopologyNode(
                  paneTree: paneTree,
                  visiblePaneIDs: Set(orderedVisiblePaneIDs)
              )
        else {
            return ShellSidebarPaneTopology(kind: .complex(count: 0), paneIDs: [])
        }

        let paneIDs = normalizedTree.paneIDs
        guard paneIDs.count > 1 else {
            return ShellSidebarPaneTopology(kind: .single, paneIDs: paneIDs)
        }

        if let splitDirection = normalizedTree.splitDirection,
           let flattened = normalizedTree.flattenedPaneIDs(along: splitDirection),
           flattened.count == paneIDs.count,
           (2...4).contains(flattened.count)
        {
            let kind: ShellSidebarPaneTopologyKind = splitDirection == .vertical
                ? .columns(count: flattened.count)
                : .rows(count: flattened.count)
            return ShellSidebarPaneTopology(kind: kind, paneIDs: flattened)
        }

        if let mainStackKind = Self.mainStackKind(for: normalizedTree) {
            return ShellSidebarPaneTopology(kind: mainStackKind, paneIDs: paneIDs)
        }

        if let gridKind = Self.gridKind(for: normalizedTree) {
            return ShellSidebarPaneTopology(kind: gridKind, paneIDs: paneIDs)
        }

        return ShellSidebarPaneTopology(kind: .complex(count: paneIDs.count), paneIDs: paneIDs)
    }

    private static func mainStackKind(
        for node: ShellSidebarPaneTopologyNode
    ) -> ShellSidebarPaneTopologyKind? {
        guard case .split(let direction, let children) = node,
              children.count == 2
        else {
            return nil
        }

        let first = children[0]
        let second = children[1]

        switch direction {
        case .vertical:
            if first.leafPaneID != nil,
               second.flattenedPaneIDs(along: .horizontal)?.count == 2
            {
                return .mainLeftWithRightStack
            }
            if first.flattenedPaneIDs(along: .horizontal)?.count == 2,
               second.leafPaneID != nil
            {
                return .mainRightWithLeftStack
            }
        case .horizontal:
            if first.leafPaneID != nil,
               second.flattenedPaneIDs(along: .vertical)?.count == 2
            {
                return .mainTopWithBottomSplit
            }
            if first.flattenedPaneIDs(along: .vertical)?.count == 2,
               second.leafPaneID != nil
            {
                return .mainBottomWithTopSplit
            }
        }

        return nil
    }

    private static func gridKind(
        for node: ShellSidebarPaneTopologyNode
    ) -> ShellSidebarPaneTopologyKind? {
        guard case .split(let direction, let children) = node,
              children.count == 2
        else {
            return nil
        }

        let childDirection: ShellSplitDirection = direction == .vertical ? .horizontal : .vertical
        guard children.allSatisfy({ $0.flattenedPaneIDs(along: childDirection)?.count == 2 }) else {
            return nil
        }

        return .grid2x2(rootDirection: direction)
    }
}

enum ShellSidebarPaneTopologyKind: Equatable {
    case single
    case columns(count: Int)
    case rows(count: Int)
    case mainLeftWithRightStack
    case mainRightWithLeftStack
    case mainTopWithBottomSplit
    case mainBottomWithTopSplit
    case grid2x2(rootDirection: ShellSplitDirection)
    case complex(count: Int)
}

private enum ShellSidebarPaneTopologyNode: Equatable {
    case pane(String)
    case split(direction: ShellSplitDirection, children: [ShellSidebarPaneTopologyNode])

    init?(
        paneTree: ShellPaneTreeNode,
        visiblePaneIDs: Set<String>
    ) {
        switch paneTree.kind {
        case .pane:
            guard let paneID = paneTree.paneID,
                  visiblePaneIDs.contains(paneID)
            else {
                return nil
            }
            self = .pane(paneID)
        case .split:
            let visibleChildren = (paneTree.children ?? []).compactMap {
                ShellSidebarPaneTopologyNode(paneTree: $0, visiblePaneIDs: visiblePaneIDs)
            }
            guard visibleChildren.count > 1,
                  let direction = paneTree.direction
            else {
                guard let onlyChild = visibleChildren.first else { return nil }
                self = onlyChild
                return
            }
            self = .split(direction: direction, children: visibleChildren)
        }
    }

    var paneIDs: [String] {
        switch self {
        case .pane(let paneID):
            return [paneID]
        case .split(_, let children):
            return children.flatMap(\.paneIDs)
        }
    }

    var splitDirection: ShellSplitDirection? {
        if case .split(let direction, _) = self {
            return direction
        }
        return nil
    }

    var leafPaneID: String? {
        if case .pane(let paneID) = self {
            return paneID
        }
        return nil
    }

    func flattenedPaneIDs(along direction: ShellSplitDirection) -> [String]? {
        switch self {
        case .pane(let paneID):
            return [paneID]
        case .split(let splitDirection, let children):
            guard splitDirection == direction else { return nil }
            var paneIDs: [String] = []
            for child in children {
                guard let childPaneIDs = child.flattenedPaneIDs(along: direction) else {
                    return nil
                }
                paneIDs.append(contentsOf: childPaneIDs)
            }
            return paneIDs
        }
    }
}
