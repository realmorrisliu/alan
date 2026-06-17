use alan_shell_core::{FixtureCorpus, FixtureKind, FixtureSource};
use serde_json::json;
use std::fs;

#[test]
fn fixture_corpus_loads_swift_exported_cases_by_stable_id() {
    let tempdir = tempfile::tempdir().unwrap();
    let fixture_dir = tempdir.path().join("split-tree");
    fs::create_dir_all(&fixture_dir).unwrap();
    fs::write(
        fixture_dir.join("simple-split.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "split-tree/simple-split",
            "kind": "split_tree",
            "source": "swift",
            "description": "Swift-exported split fixture",
            "input": {"pane_slots": 1},
            "operation": {"type": "split", "axis": "right"},
            "expected": {"pane_slots": 2}
        }))
        .unwrap(),
    )
    .unwrap();

    let corpus = FixtureCorpus::load(tempdir.path()).unwrap();
    let case = corpus.case("split-tree/simple-split").unwrap();

    assert_eq!(case.id, "split-tree/simple-split");
    assert_eq!(case.kind, FixtureKind::SplitTree);
    assert_eq!(case.source, FixtureSource::Swift);
    assert_eq!(case.input, json!({"pane_slots": 1}));
    assert_eq!(case.operation, json!({"type": "split", "axis": "right"}));
    assert_eq!(case.expected, json!({"pane_slots": 2}));
}

#[test]
fn fixture_case_compares_expected_semantics_with_actual_output() {
    let case = alan_shell_core::FixtureCase {
        id: "split-tree/simple-split".to_string(),
        kind: FixtureKind::SplitTree,
        source: FixtureSource::Swift,
        description: "Swift-exported split fixture".to_string(),
        input: json!({"pane_slots": 1}),
        operation: json!({"type": "split", "axis": "right"}),
        expected: json!({"pane_slots": 2}),
    };

    case.assert_expected_matches(&json!({"pane_slots": 2}))
        .unwrap();
    let error = case
        .assert_expected_matches(&json!({"pane_slots": 3}))
        .unwrap_err();

    assert!(error.to_string().contains("split-tree/simple-split"));
    assert!(
        error
            .to_string()
            .contains("fixture expected semantic output")
    );
}

#[test]
fn fixture_loader_rejects_file_path_and_id_mismatch() {
    let tempdir = tempfile::tempdir().unwrap();
    let fixture_dir = tempdir.path().join("manifest");
    fs::create_dir_all(&fixture_dir).unwrap();
    fs::write(
        fixture_dir.join("bad-id.json"),
        serde_json::to_vec_pretty(&json!({
            "id": "manifest/other",
            "kind": "manifest",
            "source": "swift",
            "description": "Mismatched fixture id",
            "input": {},
            "operation": {},
            "expected": {}
        }))
        .unwrap(),
    )
    .unwrap();

    let error = FixtureCorpus::load(tempdir.path()).unwrap_err();

    assert!(error.to_string().contains("fixture id"));
    assert!(error.to_string().contains("manifest/bad-id"));
}
