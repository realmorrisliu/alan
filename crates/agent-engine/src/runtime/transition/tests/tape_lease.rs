use super::*;
use alan_ap::{ErrorCode, FileKind, InProcessTransport, Qid, Stat};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountTapeOpens {
    inner: alan_agentfs::AgentFs,
    tape_path: u64,
    writes: AtomicUsize,
}

#[async_trait::async_trait]
impl FileServer for CountTapeOpens {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let qid = self.inner.open(fid, mode).await?;
        if qid.path == self.tape_path && mode == OpenMode::Write {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
        Ok(qid)
    }
    async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn approved_replay_and_resumed_generation_share_one_tape_lease() {
    for batch in [false, true] {
        let inner = alan_agentfs::AgentFs::new();
        let tape_path = inner
            .walk(Fid::ROOT, Fid(9), &["machine".into(), "tape".into()])
            .await
            .unwrap()
            .path;
        inner.clunk(Fid(9)).await.unwrap();
        let agentfs = Arc::new(CountTapeOpens {
            inner,
            tape_path,
            writes: AtomicUsize::new(0),
        });
        let procfs = Arc::new(alan_kernel::ProcFs::new());
        spawn_test_process(&procfs).await;
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(DelayedMockProvider::new(
                tokio::time::Duration::ZERO,
                "resumed answer",
            )),
        );
        let mut ns = alan_kernel::Namespace::new();
        for (path, server) in [
            ("/agent/1", InProcessTransport::new(agentfs.clone())),
            ("/proc", InProcessTransport::new(procfs)),
            ("/mnt/llm", InProcessTransport::new(llmfs)),
        ] {
            ns.mount(path, server, alan_kernel::Access::ReadWrite);
        }
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let mut state = runtime_state_with_environment(NamespaceRuntimeEnvironment::new(
            root.clone(),
            "/agent/1",
            "default",
        ));
        state.core_config.memory.enabled = false;
        state.machine.add_user_message("task");
        state.machine.begin_turn(0);
        let call = NormalizedToolCall {
            id: "call-1".into(),
            name: "update_plan".into(),
            arguments: json!({"items":[{"id":"1","content":"Done","status":"completed"}]}),
        };
        state.machine.set_confirmation(PendingConfirmation {
            checkpoint_id: "approval".into(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.into(),
            summary: "approve".into(),
            details: json!({"replay_tool_call": {
                "call_id":call.id,"tool_name":call.name,"arguments":call.arguments,
            }}),
            options: vec!["approve".into()],
        });
        if batch {
            state
                .machine
                .set_tool_replay_batch("approval", vec![call], true);
        }
        let mut emit = |_| async {};
        handle_submission_with_cancel(
            &mut state,
            Submission::new(Op::Resume {
                request_id: "approval".into(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice":"approve"}),
                )],
            }),
            &mut emit,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let tape = Shell::new(root).cat("/agent/1/machine/tape").await.unwrap();
        assert!(String::from_utf8(tape).unwrap().contains("resumed answer"));
        assert_eq!(
            agentfs.writes.load(Ordering::SeqCst),
            1,
            "replay and its generation must not release/reacquire Tape"
        );
        state
            .agent_files()
            .begin_tape_generation()
            .await
            .unwrap()
            .finish()
            .await
            .unwrap();
    }
}
