#[cfg(target_os = "macos")]
use super::HostEndpointPaths;
use super::cli::dispatch::{
    BareRunMode, Cli, Commands, bare_run_mode, resolve_no_resume_update, validate_bare_flags,
    validate_resume_scope,
};
#[cfg(target_os = "macos")]
use super::foreground::generated_foreground_runtime_dir;
use clap::Parser;

#[test]
fn hidden_tui_backend_flag_is_unavailable() {
    let err = Cli::try_parse_from(["alan", "--tui-backend", "network"])
        .map(|_| ())
        .unwrap_err();
    assert!(err.to_string().contains("--tui-backend"));
}

#[test]
fn bare_mode_uses_redirected_stdin_as_the_one_shot_boundary() {
    assert_eq!(
        bare_run_mode(true, true, None, false, vec![]).unwrap(),
        BareRunMode::Interactive { images: vec![] }
    );
    assert_eq!(
        bare_run_mode(false, true, None, false, vec![]).unwrap(),
        BareRunMode::OneShot {
            prompt: None,
            json: false,
            images: vec![]
        }
    );
    assert_eq!(
        bare_run_mode(true, true, Some("hi".into()), true, vec![]).unwrap(),
        BareRunMode::OneShot {
            prompt: Some("hi".into()),
            json: true,
            images: vec![]
        }
    );
    let err = bare_run_mode(true, false, None, false, vec![]).unwrap_err();
    assert!(err.to_string().contains("needs terminal stdout"));
    assert!(bare_run_mode(true, true, None, true, vec![]).is_err());
}

#[test]
fn shadow_evaluator_requires_an_explicit_nonempty_profile() {
    assert!(
        Cli::try_parse_from(["alan"])
            .unwrap()
            .shadow_evaluator
            .is_none()
    );
    assert_eq!(
        Cli::try_parse_from(["alan", "--shadow-evaluator", "evaluation"])
            .unwrap()
            .shadow_evaluator
            .as_deref(),
        Some("evaluation")
    );
    assert!(Cli::try_parse_from(["alan", "--shadow-evaluator", ""]).is_err());
}

#[test]
fn bare_resume_is_explicit_and_cannot_be_combined_with_a_subcommand() {
    assert!(Cli::try_parse_from(["alan"]).unwrap().resume.is_none());
    assert_eq!(
        Cli::try_parse_from(["alan", "--resume"])
            .unwrap()
            .resume
            .as_deref(),
        Some("")
    );
    assert_eq!(
        Cli::try_parse_from(["alan", "--resume", "rollout-abc"])
            .unwrap()
            .resume
            .as_deref(),
        Some("rollout-abc")
    );
    assert!(validate_resume_scope(true, true).is_err());
    assert!(validate_resume_scope(true, false).is_ok());
}

#[test]
fn trust_posture_flags_enforce_explicit_invocation_only_hierarchy() {
    let defaults = Cli::try_parse_from(["alan"]).unwrap();
    let resolved_default =
        super::cli::trust::resolve_bare_trust_options(&defaults.trust, false).unwrap();
    assert_eq!(
        resolved_default.posture,
        alan_agent_engine::TrustPosture::Governed
    );

    let confined = Cli::try_parse_from(["alan", "--trust", "--guardian-profile", "fast"]).unwrap();
    let resolved_confined =
        super::cli::trust::resolve_bare_trust_options(&confined.trust, false).unwrap();
    assert_eq!(
        resolved_confined.posture,
        alan_agent_engine::TrustPosture::ConfinedTrusted
    );
    assert_eq!(resolved_confined.guardian_profile.as_deref(), Some("fast"));

    let unsafe_only = Cli::try_parse_from(["alan", "--unsafe-host-account"]).unwrap();
    assert!(super::cli::trust::resolve_bare_trust_options(&unsafe_only.trust, false).is_err());

    let unsafe_with_trust =
        Cli::try_parse_from(["alan", "--trust", "--unsafe-host-account"]).unwrap();
    assert_eq!(
        super::cli::trust::resolve_bare_trust_options(&unsafe_with_trust.trust, false)
            .unwrap()
            .posture,
        alan_agent_engine::TrustPosture::HostAccountTrusted
    );
}

#[cfg(target_os = "macos")]
#[test]
fn foreground_runtime_falls_back_when_tempdir_exceeds_socket_path_limit() {
    let long_tempdir = std::path::Path::new("/").join("x".repeat(100));
    let runtime_dir = generated_foreground_runtime_dir(&long_tempdir).unwrap();

    assert_eq!(runtime_dir.parent(), Some(std::path::Path::new("/tmp")));
    assert!(HostEndpointPaths::from_runtime_dir(&runtime_dir).is_ok());
}

#[test]
fn host_start_is_rejected_as_a_retired_background_command() {
    let error = Cli::try_parse_from(["alan", "host", "start"])
        .map(|_| ())
        .unwrap_err();
    assert!(error.to_string().contains("unrecognized subcommand"));
}

#[test]
fn memory_and_workflow_subcommands_parse_cleanly() {
    let mem =
        Cli::try_parse_from(["alan", "memory", "list", "--domain", "personal", "--json"]).unwrap();
    assert!(matches!(mem.command, Some(Commands::Memory(_))));

    let wf = Cli::try_parse_from(["alan", "workflow", "list", "--json"]).unwrap();
    assert!(matches!(wf.command, Some(Commands::Workflow(_))));
}

#[test]
fn login_subcommand_and_no_auto_mount_flag_parse_cleanly() {
    let login = Cli::try_parse_from(["alan", "login", "openrouter", "--no-browser"]).unwrap();
    assert!(matches!(
        login.command,
        Some(Commands::Login {
            provider: Some(_),
            no_browser: true,
            profile: None,
        })
    ));

    let chatgpt = Cli::try_parse_from(["alan", "login", "chatgpt", "--no-browser"]).unwrap();
    assert!(matches!(
        chatgpt.command,
        Some(Commands::Login {
            no_browser: true,
            ..
        })
    ));

    let bare = Cli::try_parse_from(["alan", "--no-auto-mount"]).unwrap();
    assert!(bare.no_auto_mount);
    assert!(bare.command.is_none());
}

#[test]
fn update_subcommand_parses_all_flags_cleanly() {
    let parsed = Cli::try_parse_from([
        "alan",
        "update",
        "--check",
        "--json",
        "--version",
        "v0.2.0",
        "--force",
        "--install-dir",
        "/tmp/alan-bin",
        "--manifest-url",
        "http://127.0.0.1:8080/latest.json",
        "--quiet",
    ])
    .unwrap();
    match parsed.command {
        Some(Commands::Update(args)) => {
            assert!(args.check);
            assert!(args.json);
            assert_eq!(args.version.as_deref(), Some("v0.2.0"));
            assert!(args.force);
            assert_eq!(
                args.install_dir.as_deref(),
                Some(std::path::Path::new("/tmp/alan-bin"))
            );
            assert_eq!(
                args.manifest_url.as_deref(),
                Some("http://127.0.0.1:8080/latest.json")
            );
            assert!(args.quiet);
        }
        _ => panic!("expected Commands::Update"),
    }
    assert!(!parsed.json);
}

#[test]
fn no_resume_update_flag_and_env_parity_are_validated() {
    let bare = Cli::try_parse_from(["alan", "--no-resume-update"]).unwrap();
    assert!(bare.no_resume_update);
    assert!(
        validate_bare_flags(false, false, bare.no_resume_update, None, false).is_ok(),
        "--no-resume-update should be allowed on bare alan"
    );
    assert!(
        validate_bare_flags(true, false, true, None, false).is_ok(),
        "--no-resume-update should be allowed alongside --resume"
    );
    assert!(
        validate_bare_flags(false, true, true, None, false).is_ok(),
        "--no-resume-update should be allowed alongside --ephemeral"
    );
    assert!(
        validate_bare_flags(false, false, true, None, true).is_err(),
        "--no-resume-update with subcommand must fail"
    );

    assert!(!resolve_no_resume_update(false, None));
    assert!(!resolve_no_resume_update(false, Some("")));
    assert!(!resolve_no_resume_update(false, Some("0")));
    assert!(!resolve_no_resume_update(false, Some("false")));
    assert!(!resolve_no_resume_update(false, Some("FALSE")));
    assert!(resolve_no_resume_update(false, Some("1")));
    assert!(resolve_no_resume_update(false, Some("true")));
    assert!(resolve_no_resume_update(true, None));
    assert!(resolve_no_resume_update(true, Some("0")));
}
