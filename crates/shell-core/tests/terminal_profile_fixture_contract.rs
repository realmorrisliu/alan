use alan_shell_core::{
    FixtureCase, FixtureCorpus, FixtureKind, FixtureSource, ManagedTerminalAccountFakeExecutor,
    ManagedTerminalAccountPlanner, ManagedTerminalAccountProfileHandoff,
    ManagedTerminalAccountRequest, ManagedTerminalAccountState, ManagedTerminalAccountSudoersRule,
    TerminalProfileDocument, TerminalProfileEditor, TerminalProfileEditorDraft,
    TerminalProfileValidator,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

const SWIFT_TERMINAL_PROFILE_FIXTURES: &[&str] = &[
    "terminal-profile/validation-errors",
    "terminal-profile/editor-make-definition",
    "terminal-profile/managed-account-dev-dry-run",
];

#[test]
fn rust_terminal_profile_domain_matches_swift_exported_fixtures() {
    let corpus = FixtureCorpus::load(fixtures_root()).expect("fixtures load");

    for fixture_id in SWIFT_TERMINAL_PROFILE_FIXTURES {
        let case = corpus
            .case(fixture_id)
            .unwrap_or_else(|| panic!("missing Swift terminal profile fixture {fixture_id}"));
        assert_eq!(case.kind, FixtureKind::TerminalProfile);
        assert_eq!(case.source, FixtureSource::Swift);

        let actual = apply_terminal_profile_fixture(case);
        case.assert_expected_matches(&actual)
            .unwrap_or_else(|error| panic!("{fixture_id}: {error}"));
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn apply_terminal_profile_fixture(case: &FixtureCase) -> Value {
    let operation: TerminalProfileFixtureOperation =
        serde_json::from_value(case.operation.clone()).expect("terminal profile fixture operation");

    match operation {
        TerminalProfileFixtureOperation::Validate => {
            let document: TerminalProfileDocument = serde_json::from_value(case.input.clone())
                .expect("terminal profile fixture document");
            let result = TerminalProfileValidator::validate(&document);
            json!({
                "is_valid": result.is_valid(),
                "errors": result.errors,
            })
        }
        TerminalProfileFixtureOperation::MakeDefinition { draft } => {
            let result = TerminalProfileEditor::make_definition(draft);
            json!({
                "is_valid": result.is_valid(),
                "definition": result.definition,
                "errors": result.errors,
            })
        }
        TerminalProfileFixtureOperation::ManagedAccountDryRun => {
            let input: ManagedAccountDryRunFixtureInput =
                serde_json::from_value(case.input.clone()).expect("managed account fixture input");
            let plan =
                ManagedTerminalAccountPlanner::plan(input.request.clone(), &input.missing_state);
            let cancelled_apply_result =
                ManagedTerminalAccountFakeExecutor::apply(&plan, input.cancel_before_apply, None);
            let profile_handoff = ManagedTerminalAccountProfileHandoff::profile_definition(
                &input.request,
                &input.ready_state,
            );
            json!({
                "plan": {
                    "status": plan.status.label(),
                    "steps": plan.steps,
                },
                "sudoers_rule": ManagedTerminalAccountSudoersRule::new(&input.request),
                "cancelled_apply_result": cancelled_apply_result,
                "profile_handoff": profile_handoff,
            })
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalProfileFixtureOperation {
    Validate,
    MakeDefinition { draft: TerminalProfileEditorDraft },
    ManagedAccountDryRun,
}

#[derive(Debug, Deserialize)]
struct ManagedAccountDryRunFixtureInput {
    request: ManagedTerminalAccountRequest,
    missing_state: ManagedTerminalAccountState,
    ready_state: ManagedTerminalAccountState,
    cancel_before_apply: bool,
}
