use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use alan_agent_protocol::{ContentPart, InputMode, Op};
use alan_agentfs::{AgentConformanceChecker, AgentFs, AgentRootFs};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Request, Stat,
};
use alan_kernel::{
    Access, Credentials, MountFs, Namespace, ProcFs, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk};
use alan_llmfs::LlmFs;
use alan_shell::Shell;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::client::NamespaceClient;
use super::*;

struct EchoRunner;

#[async_trait::async_trait]
impl ProcessRunner for EchoRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        let Ok(resolved) = invocation.namespace.resolve(&invocation.exec.executable) else {
            return ProcessOutcome::exited(127, b"executable is not mounted\n".to_vec());
        };
        let fid = Fid(60_000 + invocation.pid.0);
        let reachable = resolved
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: resolved.rel.clone(),
            })
            .await
            .is_ok();
        let _ = resolved.call(Request::Clunk { fid }).await;
        if !reachable {
            return ProcessOutcome::exited(127, b"executable is not reachable\n".to_vec());
        }
        let mut output = invocation.exec.args.join(" ").into_bytes();
        output.push(b'\n');
        ProcessOutcome::exited(0, output)
    }
}

struct LargeOutputRunner;

#[async_trait::async_trait]
impl ProcessRunner for LargeOutputRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        ProcessOutcome::exited(0, vec![b'x'; 70 * 1024])
    }
}

struct ExactChunkOutputRunner;

#[async_trait::async_trait]
impl ProcessRunner for ExactChunkOutputRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        ProcessOutcome::exited(0, vec![b'y'; 64 * 1024])
    }
}

struct LogicalFailureRunner;

#[async_trait::async_trait]
impl ProcessRunner for LogicalFailureRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        ProcessOutcome::exited(
            0,
            b"{\"success\":false,\"exit_code\":2,\"error\":\"command failed\"}\n".to_vec(),
        )
    }
}

struct AbortObservedRunner {
    started: Arc<Notify>,
    dropped: Arc<Notify>,
}

struct AbortDropGuard {
    dropped: Arc<Notify>,
}

impl Drop for AbortDropGuard {
    fn drop(&mut self) {
        self.dropped.notify_one();
    }
}

#[async_trait::async_trait]
impl ProcessRunner for AbortObservedRunner {
    async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
        let _guard = AbortDropGuard {
            dropped: Arc::clone(&self.dropped),
        };
        self.started.notify_one();
        std::future::pending::<ProcessOutcome>().await
    }
}

struct BlockingStreamProvider {
    started: Arc<Notify>,
}

#[async_trait::async_trait]
impl LlmProvider for BlockingStreamProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        Err(anyhow::anyhow!("blocking provider uses streaming"))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok("blocking stream provider".to_string())
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.started.notify_one();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _hold = tx;
            std::future::pending::<()>().await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "blocking_stream"
    }
}

fn tool_test_environment(runner: Arc<dyn ProcessRunner>) -> (NamespaceRuntimeEnvironment, Shell) {
    let procfs = ProcFs::new().with_runner(runner);
    let agentfs = Arc::new(AgentFs::new());
    let binfs = Arc::new(alan_ap::reference::MemFs::new());

    let mut child_namespace = Namespace::new();
    child_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    child_namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs.clone()),
        Access::ReadWrite,
    );
    child_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);

    let spawner_procfs =
        Arc::new(procfs.for_spawner(None, child_namespace, Credentials::user("root-agent")));
    let mut root_namespace = Namespace::new();
    root_namespace.mount(
        "/proc",
        InProcessTransport::new(spawner_procfs),
        Access::ReadWrite,
    );
    root_namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
    (
        NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default"),
        Shell::new(root),
    )
}

struct BlockingReadFs {
    read_started: Notify,
    clunked: Notify,
    clunk_count: AtomicUsize,
}

impl BlockingReadFs {
    fn new() -> Self {
        Self {
            read_started: Notify::new(),
            clunked: Notify::new(),
            clunk_count: AtomicUsize::new(0),
        }
    }

    fn qid(kind: FileKind) -> Qid {
        Qid {
            kind,
            version: 0,
            path: 1,
        }
    }
}

#[async_trait::async_trait]
impl FileServer for BlockingReadFs {
    async fn walk(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _names: &[String],
    ) -> std::result::Result<Qid, ErrorCode> {
        Ok(Self::qid(FileKind::Stream))
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        Ok(Self::qid(FileKind::Stream))
    }

    async fn read(
        &self,
        _fid: Fid,
        _offset: u64,
        _count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        self.read_started.notify_one();
        std::future::pending().await
    }

    async fn write(
        &self,
        _fid: Fid,
        _offset: u64,
        _data: &[u8],
    ) -> std::result::Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> std::result::Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> std::result::Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
        self.clunk_count.fetch_add(1, AtomicOrdering::SeqCst);
        self.clunked.notify_one();
        Ok(())
    }
}

struct ScriptedReadFs {
    data: Vec<u8>,
    kind: FileKind,
    stat_length: Option<u64>,
    clunk_count: AtomicUsize,
}

impl ScriptedReadFs {
    fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            kind: FileKind::Stream,
            stat_length: None,
            clunk_count: AtomicUsize::new(0),
        }
    }

    fn shrinking_file(data: impl Into<Vec<u8>>, reported_length: u64) -> Self {
        Self {
            data: data.into(),
            kind: FileKind::File,
            stat_length: Some(reported_length),
            clunk_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl FileServer for ScriptedReadFs {
    async fn walk(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _names: &[String],
    ) -> std::result::Result<Qid, ErrorCode> {
        Ok(BlockingReadFs::qid(self.kind))
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        Ok(BlockingReadFs::qid(self.kind))
    }

    async fn read(
        &self,
        _fid: Fid,
        offset: u64,
        count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        let start = offset as usize;
        if start >= self.data.len() {
            return Ok(Vec::new());
        }
        let end = (start + count as usize).min(self.data.len());
        Ok(self.data[start..end].to_vec())
    }

    async fn write(
        &self,
        _fid: Fid,
        _offset: u64,
        _data: &[u8],
    ) -> std::result::Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> std::result::Result<Stat, ErrorCode> {
        self.stat_length
            .map(|length| Stat {
                name: "status".to_string(),
                qid: BlockingReadFs::qid(self.kind),
                length,
                executable: false,
                writable: false,
            })
            .ok_or(ErrorCode::Unsupported)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> std::result::Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
        self.clunk_count.fetch_add(1, AtomicOrdering::SeqCst);
        Ok(())
    }
}

mod agent_files;
mod client_io;
mod process_actions;
mod submissions;
mod tape;

#[tokio::test]
async fn answered_request_response_resumes_engine_pending_yield_from_files() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
    let agent_files = environment.agent_files();

    let request_id = agent_files
        .write_request(NamespaceRequestRecord::new(
            "structured_input",
            "Provide the missing detail",
        ))
        .await
        .unwrap();
    assert_eq!(request_id, "r0");
    assert!(
        agent_files
            .resume_submission_from_answered_request(&request_id)
            .await
            .unwrap()
            .is_none()
    );

    shell
        .write(
            &format!("/agent/1/requests/{request_id}/response"),
            br#"{"answers":[{"question_id":"q1","value":"answer from request file"}]}"#,
        )
        .await
        .unwrap();
    let submission = agent_files
        .resume_submission_from_answered_request(&request_id)
        .await
        .unwrap()
        .expect("answered request becomes a resume submission");
    match &submission.op {
        Op::Resume {
            request_id: resumed_id,
            content,
        } => {
            assert_eq!(resumed_id, "r0");
            assert_eq!(
                content,
                &vec![ContentPart::structured(serde_json::json!({
                    "answers": [{"question_id": "q1", "value": "answer from request file"}]
                }))]
            );
        }
        other => panic!("expected Op::Resume, got {other:?}"),
    }

    let mut machine = crate::agent_machine::AgentMachine::new();
    machine.set_structured_input(crate::approval::PendingStructuredInputRequest {
        request_id,
        title: "Missing detail".to_string(),
        prompt: "Provide the missing detail".to_string(),
        questions: Vec::new(),
    });
    let mut state = super::super::RuntimeLoopState {
        machine,
        environment,
        core_config: crate::Config::default(),
        runtime_config: super::super::super::RuntimeConfig::default(),
        definition_persona_dirs: Vec::new(),
        prompt_cache: super::super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event| {
        events.push(event);
        async {}
    };

    let action = super::super::super::submission_handlers::handle_runtime_op_with_cancel(
        &mut state,
        submission.op,
        &mut emit,
        &cancel,
    )
    .await
    .unwrap();

    assert!(
        matches!(
            action,
            super::super::super::submission_handlers::RuntimeOpAction::RunTurn { .. }
        ),
        "resume should re-enter the turn path: {action:?}"
    );
    assert!(!state.machine.has_pending_interaction());
    assert!(events.is_empty());
    let messages = state.machine.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tool_responses()[0].id, "r0");
    assert_eq!(
        messages[0].tool_responses()[0].text_content(),
        r#"{"answers":[{"question_id":"q1","value":"answer from request file"}]}"#
    );
}
