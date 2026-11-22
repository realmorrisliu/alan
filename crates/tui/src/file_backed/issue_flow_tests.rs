use std::sync::{Arc, Mutex};

use alan_agent_protocol::{
    ReasoningControls, ReasoningEffort, UiModelBinding, UiModelControlSource, UiModelSnapshot,
};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use alan_kernel::{Access, MountFs, Namespace};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::diagnostics::MAX_FALLBACK_URL_BYTES;
use super::*;
use crate::file_backed::{ProjectAccess, ProjectMountReceipt};
use crate::history::HistoryCell;

fn test_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

struct MockGithubFs {
    writable: bool,
    writes: Mutex<Vec<Vec<u8>>>,
    receipt: Vec<u8>,
}

#[async_trait::async_trait]
impl FileServer for MockGithubFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names.join("/").as_str() {
            ""
            | "repos"
            | "repos/realmorrisliu"
            | "repos/realmorrisliu/alan"
            | "repos/realmorrisliu/alan/issues" => Ok(Qid {
                kind: FileKind::Dir,
                path: 1,
                version: 0,
            }),
            "repos/realmorrisliu/alan/issues/new" => Ok(Qid {
                kind: FileKind::File,
                path: 2,
                version: 0,
            }),
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, _fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if !self.writable && matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        Ok(Qid {
            kind: FileKind::File,
            path: 2,
            version: 0,
        })
    }

    async fn read(&self, _fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let start = (offset as usize).min(self.receipt.len());
        let end = (start + count as usize).min(self.receipt.len());
        Ok(self.receipt[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.writes.lock().unwrap().push(data.to_vec());
        Ok(data.len() as u32)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: "new".to_string(),
            qid: Qid {
                kind: FileKind::File,
                path: 2,
                version: 0,
            },
            length: self.receipt.len() as u64,
            executable: false,
            writable: self.writable,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[test]
fn redaction_excludes_transcripts_source_contents_and_secrets() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.project = Some(ProjectMountReceipt {
        grant_id: "grant-99".into(),
        namespace_path: "/mnt/project-request-abc".into(),
        label: "top-secret-client".into(),
        access: ProjectAccess::ReadOnly,
    });
    app.project_host_root = Some(std::path::PathBuf::from("/Users/alice/top-secret-client"));
    app.sandbox_backend = Some("seatbelt".into());
    app.trust_posture_label = Some("auto-approve".into());
    app.model.apply(
        "/agent/root",
        Some(UiModelSnapshot {
            known: true,
            publication_version: 1,
            process_path: "/proc/root".into(),
            selected_next: Some(UiModelBinding {
                profile: "openai-main".into(),
                provider: "openai_responses".into(),
                model: "gpt-5.4".into(),
                reasoning: ReasoningControls {
                    effort: Some(ReasoningEffort::High),
                },
                control_source: UiModelControlSource::ProviderDefault,
            }),
            ..UiModelSnapshot::default()
        }),
    );
    app.transcript
        .push(HistoryCell::User("TRANSCRIPT_USER_SECRET".into()));
    app.transcript
        .push(HistoryCell::Assistant("TRANSCRIPT_ASSISTANT_SECRET".into()));
    app.transcript.push(HistoryCell::Error(
        "Auth error with Bearer ghp_1234567890abcdef OPENAI_API_KEY=sk-proj-secret12345 Authorization: Bearer topsecret99 api_key: customsecret at /Users/alice/private/main.rs and https://bob:pw@github.com/org/repo".into(),
    ));

    app.handle_command("/issue Broken login");
    let rendered = render_lines(&app, 100)
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Broken login"));
    assert!(rendered.contains(env!("CARGO_PKG_VERSION")));
    assert!(rendered.contains(std::env::consts::OS));
    assert!(rendered.contains("openai_responses / gpt-5.4 (high)"));
    assert!(rendered.contains("auto-approve (sandbox: seatbelt, mount: read-only)"));
    assert!(!rendered.contains("TRANSCRIPT_USER_SECRET"));
    assert!(!rendered.contains("TRANSCRIPT_ASSISTANT_SECRET"));
    assert!(!rendered.contains("ghp_1234567890abcdef"));
    assert!(!rendered.contains("sk-proj-secret12345"));
    assert!(!rendered.contains("topsecret99"));
    assert!(!rendered.contains("customsecret"));
    assert!(!rendered.contains("bob:pw"));
    assert!(!rendered.contains("/Users/alice"));
    assert!(!rendered.contains("/mnt/project-request-abc"));
}

#[test]
fn help_notice_is_not_captured_as_diagnostic_error_summary() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.handle_command("/help");
    app.handle_command("/issue Check help filter");
    let rendered = render_lines(&app, 100)
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Latest error/notice: none"), "{rendered}");
}

#[test]
fn issue_and_bug_commands_require_confirm_edit_and_cancel_safely() {
    let mut app = FileBackedApp::new("/agent/root".into());
    assert!(
        app.completion_sources
            .commands
            .iter()
            .any(|c| c.value == "issue")
    );
    assert!(
        app.completion_sources
            .commands
            .iter()
            .any(|c| c.value == "bug")
    );

    assert!(app.handle_command("/bug").is_none());
    assert!(app.issue.is_active());
    // Empty title blocks Enter submit safely.
    assert!(app.handle_key(test_key(KeyCode::Enter)).is_none());
    assert!(app.issue.is_active());
    // Esc cancels with zero actions emitted.
    assert!(app.handle_key(test_key(KeyCode::Esc)).is_none());
    assert!(!app.issue.is_active());
    assert_eq!(app.notice.as_deref(), Some("Issue draft cancelled."));

    // With title and edits, Enter emits SubmitIssue.
    app.handle_command("/issue Title1");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
    app.handle_key(test_key(KeyCode::Char('X')));
    app.handle_key(test_key(KeyCode::Tab)); // Title
    app.handle_key(test_key(KeyCode::Char('!')));
    let action = app.handle_key(test_key(KeyCode::Enter));
    let Some(FileBackedAction::SubmitIssue {
        generation,
        ref title,
        ref body,
        ..
    }) = action
    else {
        panic!("expected SubmitIssue");
    };
    assert_eq!(title, "Title1!");
    assert!(body.contains("X"), "{body}");

    // Cancelling while Submitting ignores late completion results via generation guard.
    assert!(matches!(
        app.handle_key(test_key(KeyCode::Esc)),
        Some(FileBackedAction::CancelIssue)
    ));
    assert!(!app.issue.is_active());
    let transcript_before = app.transcript.len();
    app.apply_issue_submit_result(
        generation,
        Ok(IssueSubmissionOutcome::GhCli {
            number: Some(55),
            url: "https://github.com/realmorrisliu/alan/issues/55".into(),
        }),
    );
    assert_eq!(app.transcript.len(), transcript_before);
}

#[tokio::test]
async fn tiered_submission_prefers_writable_githubfs_then_gh_then_web_url() {
    let ghfs = Arc::new(MockGithubFs {
        writable: true,
        writes: Mutex::new(Vec::new()),
        receipt: br#"{"number":42,"url":"https://github.com/realmorrisliu/alan/issues/42"}"#
            .to_vec(),
    });
    let mount = Arc::new(MountFs::new(Namespace::new()));
    mount.live_namespace().mount(
        "/mnt/github",
        alan_ap::InProcessTransport::new(ghfs.clone()),
        Access::ReadWrite,
    );
    let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(mount));
    let out = submit_issue_tiered(&shell, None, "realmorrisliu/alan", "Bug", "Body")
        .await
        .unwrap();
    assert_eq!(
        out,
        IssueSubmissionOutcome::MountedGithubFs {
            number: Some(42),
            url: "https://github.com/realmorrisliu/alan/issues/42".into(),
        }
    );
    assert_eq!(ghfs.writes.lock().unwrap().len(), 1);

    // ReadOnly /mnt/github falls through to Tier 2 gh handler.
    let ro_mount = Arc::new(MountFs::new(Namespace::new()));
    ro_mount.live_namespace().mount(
        "/mnt/github",
        alan_ap::InProcessTransport::new(ghfs),
        Access::ReadOnly,
    );
    let ro_shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(ro_mount));
    let gh_handler: IssueControlHandler = Arc::new(|_| {
        Box::pin(async {
            Ok(IssueSubmitResult::CreatedGhCli {
                number: Some(99),
                url: "https://github.com/realmorrisliu/alan/issues/99".into(),
            })
        })
    });
    let out2 = submit_issue_tiered(
        &ro_shell,
        Some(gh_handler),
        "realmorrisliu/alan",
        "Bug",
        "Body",
    )
    .await
    .unwrap();
    assert_eq!(
        out2,
        IssueSubmissionOutcome::GhCli {
            number: Some(99),
            url: "https://github.com/realmorrisliu/alan/issues/99".into(),
        }
    );

    // Unavailable gh falls back to bounded prefilled URL (UTF-8 safe).
    let unavail: IssueControlHandler = Arc::new(|_| {
        Box::pin(async {
            Ok(IssueSubmitResult::GhUnavailable {
                reason: "not logged in".into(),
            })
        })
    });
    let long_body = "中文🙂".repeat(1000);
    let out3 = submit_issue_tiered(
        &ro_shell,
        Some(unavail),
        "realmorrisliu/alan",
        "Bug 1",
        &long_body,
    )
    .await
    .unwrap();
    match out3 {
        IssueSubmissionOutcome::WebFallback { url, reason } => {
            assert_eq!(reason, "not logged in");
            assert!(url.starts_with(
                "https://github.com/realmorrisliu/alan/issues/new?title=Bug%201&body="
            ));
            assert!(url.len() <= MAX_FALLBACK_URL_BYTES);
            assert!(url.ends_with("%0A...%28truncated%29"));
        }
        other => panic!("expected web fallback, got {other:?}"),
    }
}
