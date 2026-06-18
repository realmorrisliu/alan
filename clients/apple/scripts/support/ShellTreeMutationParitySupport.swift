extension ShellPaneTreeNode {
    func resizingSplit(
        _ targetNodeID: String,
        ratio targetRatio: Double
    ) -> (node: ShellPaneTreeNode, changed: Bool) {
        if kind == .split, nodeID == targetNodeID {
            return (
                ShellPaneTreeNode(
                    nodeID: nodeID,
                    kind: kind,
                    direction: direction,
                    ratio: targetRatio,
                    paneID: paneID,
                    children: children
                ),
                true
            )
        }

        guard let children, !children.isEmpty else { return (self, false) }
        var didChange = false
        let nextChildren = children.map { child in
            let result = child.resizingSplit(targetNodeID, ratio: targetRatio)
            didChange = didChange || result.changed
            return result.node
        }

        guard didChange else { return (self, false) }
        return (
            ShellPaneTreeNode(
                nodeID: nodeID,
                kind: kind,
                direction: direction,
                ratio: ratio,
                paneID: paneID,
                children: nextChildren
            ),
            true
        )
    }

    func equalizedSplits() -> ShellPaneTreeNode {
        switch kind {
        case .pane:
            return self
        case .split:
            return ShellPaneTreeNode(
                nodeID: nodeID,
                kind: kind,
                direction: direction,
                ratio: 0.5,
                paneID: paneID,
                children: children?.map { $0.equalizedSplits() }
            )
        }
    }

    func splittingPane(
        _ targetPaneID: String,
        direction: ShellSplitDirection,
        splitNodeID: String,
        newLeafNodeID: String,
        newPaneID: String
    ) -> ShellPaneTreeNode {
        splittingPane(
            targetPaneID,
            placement: .defaultPlacement(for: direction),
            splitNodeID: splitNodeID,
            newLeafNodeID: newLeafNodeID,
            newPaneID: newPaneID
        )
    }

    func splittingPane(
        _ targetPaneID: String,
        placement: ShellPaneSplitDirection,
        splitNodeID: String,
        newLeafNodeID: String,
        newPaneID: String
    ) -> ShellPaneTreeNode {
        switch kind {
        case .pane:
            guard paneID == targetPaneID else { return self }
            let currentLeaf = ShellPaneTreeNode(
                nodeID: nodeID,
                kind: .pane,
                direction: nil,
                paneID: targetPaneID,
                children: nil
            )
            let newLeaf = ShellPaneTreeNode(
                nodeID: newLeafNodeID,
                kind: .pane,
                direction: nil,
                paneID: newPaneID,
                children: nil
            )
            return ShellPaneTreeNode(
                nodeID: splitNodeID,
                kind: .split,
                direction: placement.splitDirection,
                ratio: 0.5,
                paneID: nil,
                children: placement.placesNewPaneBeforeTarget
                    ? [newLeaf, currentLeaf]
                    : [currentLeaf, newLeaf]
            )
        case .split:
            return ShellPaneTreeNode(
                nodeID: nodeID,
                kind: .split,
                direction: self.direction,
                ratio: ratio,
                paneID: nil,
                children: (children ?? []).map {
                    $0.splittingPane(
                        targetPaneID,
                        placement: placement,
                        splitNodeID: splitNodeID,
                        newLeafNodeID: newLeafNodeID,
                        newPaneID: newPaneID
                    )
                }
            )
        }
    }

    func removingPane(_ targetPaneID: String) -> ShellPaneTreeNode? {
        switch kind {
        case .pane:
            return paneID == targetPaneID ? nil : self
        case .split:
            let remainingChildren = (children ?? []).compactMap { $0.removingPane(targetPaneID) }
            if remainingChildren.isEmpty {
                return nil
            }
            if remainingChildren.count == 1 {
                return remainingChildren[0]
            }
            return ShellPaneTreeNode(
                nodeID: nodeID,
                kind: .split,
                direction: direction,
                ratio: ratio,
                paneID: nil,
                children: remainingChildren
            )
        }
    }

    func attachingPane(
        _ newPaneID: String,
        direction: ShellSplitDirection,
        splitNodeID: String,
        newLeafNodeID: String
    ) -> ShellPaneTreeNode {
        let newLeaf = ShellPaneTreeNode(
            nodeID: newLeafNodeID,
            kind: .pane,
            direction: nil,
            paneID: newPaneID,
            children: nil
        )

        if kind == .split,
           self.direction == direction,
           let existingChildren = children,
           let lastChild = existingChildren.last {
            let nestedSplit = ShellPaneTreeNode(
                nodeID: splitNodeID,
                kind: .split,
                direction: direction,
                ratio: 0.5,
                paneID: nil,
                children: [lastChild, newLeaf]
            )

            return ShellPaneTreeNode(
                nodeID: nodeID,
                kind: .split,
                direction: direction,
                ratio: ratio,
                paneID: nil,
                children: Array(existingChildren.dropLast()) + [nestedSplit]
            )
        }

        return ShellPaneTreeNode(
            nodeID: splitNodeID,
            kind: .split,
            direction: direction,
            ratio: 0.5,
            paneID: nil,
            children: [self, newLeaf]
        )
    }

}
