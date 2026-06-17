use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, PaneTreeNode,
    PaneTreeNodeResizeOutcome, SpatialFocusDirection, SplitPlacement,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_SPLIT_TREE_FIXTURES: &[&str] = &[
    "split-tree/split-pane-placement-right",
    "split-tree/split-pane-placement-left",
    "split-tree/resize-clamps-to-minimum",
    "split-tree/resize-clamps-to-maximum",
    "split-tree/equalize-restores-nested-ratios",
    "split-tree/zoom-leaf-preserves-canonical-tree",
    "split-tree/spatial-focus-right-preserves-row",
    "split-tree/spatial-focus-down-preserves-column",
];

#[test]
fn rust_split_tree_behavior_matches_swift_exported_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_SPLIT_TREE_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift split tree fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::SplitTree);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_split_tree_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_split_tree_fixture(case: &FixtureCase) -> Value {
    let input: SplitTreeInput =
        serde_json::from_value(case.input.clone()).expect("split tree fixture input");
    let operation: SplitTreeOperation =
        serde_json::from_value(case.operation.clone()).expect("split tree fixture operation");

    match operation {
        SplitTreeOperation::SplitPane {
            target_pane_id,
            placement,
            split_node_id,
            new_leaf_node_id,
            new_pane_id,
        } => {
            let tree = input.tree.split_pane(
                &target_pane_id,
                placement,
                split_node_id,
                new_leaf_node_id,
                new_pane_id,
            );
            json!({
                "tree": tree,
                "pane_ids": tree.pane_ids(),
            })
        }
        SplitTreeOperation::ResizeSplit {
            split_node_id,
            ratio,
        } => {
            let result = input.tree.resize_split(&split_node_id, ratio);
            json!({
                "tree": result.node,
                "outcome": match result.outcome {
                    PaneTreeNodeResizeOutcome::Changed => "changed",
                    PaneTreeNodeResizeOutcome::Unchanged => "unchanged",
                },
            })
        }
        SplitTreeOperation::EqualizeSplits => {
            let tree = input.tree.equalized_splits();
            json!({
                "tree": tree,
                "ratios_by_node_id": tree.split_ratios_by_node_id(),
            })
        }
        SplitTreeOperation::ZoomLeaf { pane_id } => {
            let leaf = input.tree.leaf_node(&pane_id).expect("zoom leaf").clone();
            json!({
                "tree": leaf,
                "canonical_tree": input.tree,
            })
        }
        SplitTreeOperation::AdjacentPane {
            target_pane_id,
            direction,
        } => {
            let pane_id = input.tree.adjacent_pane_id(&target_pane_id, direction);
            json!({
                "pane_id": pane_id,
            })
        }
    }
}

#[derive(Debug, Deserialize)]
struct SplitTreeInput {
    tree: PaneTreeNode,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SplitTreeOperation {
    SplitPane {
        target_pane_id: String,
        placement: SplitPlacement,
        split_node_id: String,
        new_leaf_node_id: String,
        new_pane_id: String,
    },
    ResizeSplit {
        split_node_id: String,
        ratio: f64,
    },
    EqualizeSplits,
    ZoomLeaf {
        pane_id: String,
    },
    AdjacentPane {
        target_pane_id: String,
        direction: SpatialFocusDirection,
    },
}
