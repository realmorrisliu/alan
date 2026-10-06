//! Repeated Process controls preserve their original selection and receipt.
use super::*;
use futures::FutureExt;

type Captures = Arc<Mutex<Vec<String>>>;

struct SelectionCatalog {
    values: Vec<CapturedCallable>,
    captures: Captures,
}
#[async_trait]
impl ConnectionAuthority for SelectionCatalog {
    async fn capture(&self, model: Option<&str>) -> Result<CapturedCallable> {
        let model = model.unwrap_or("A");
        self.captures.lock().unwrap().push(model.into());
        self.values
            .iter()
            .find(|value| value.identity.model == model)
            .cloned()
            .context("model not authorized")
    }
    async fn restore(&self, identity: &CallableIdentity) -> Result<CapturedCallable> {
        self.values
            .iter()
            .find(|value| &value.identity == identity)
            .cloned()
            .context("exact model not available")
    }
    async fn catalog(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}

#[tokio::test]
async fn model_control_delayed_retry_preserves_later_selection_and_recovery() {
    selection_retry(false, false).await;
}
#[tokio::test]
async fn model_control_failed_retry_preserves_terminal_rejection_and_recovery() {
    selection_retry(true, false).await;
}
#[tokio::test]
async fn model_control_lost_ui_receipt_retry_republishes_without_selection() {
    selection_retry(false, true).await;
}

async fn selection_retry(failed: bool, drop_receipt: bool) {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let first = Submission::new(Op::SelectModel {
        model: if failed { "missing" } else { "B" }.into(),
    });
    let later = Submission::new(Op::SelectModel { model: "C".into() });
    let expected = if failed {
        alan_agent_protocol::UiInputStatus::Failed
    } else {
        alan_agent_protocol::UiInputStatus::Completed
    };
    let (mut runtime, shell, captures, c, fault) =
        selection_runtime(stores.clone(), None, drop_receipt.then(|| first.id.clone()));
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let observed = std::panic::AssertUnwindSafe(async {
        runtime
            .handle
            .submission_tx
            .send(first.clone())
            .await
            .unwrap();
        if !drop_receipt {
            wait_receipt(&shell, &first.id, 1, expected).await;
        }
        runtime.handle.submission_tx.send(later).await.unwrap();
        // Completion of the following serialized control proves first has settled,
        // including when its first UI completion could not be written.
        let last = Submission::new(Op::SelectModel { model: "C".into() });
        runtime
            .handle
            .submission_tx
            .send(last.clone())
            .await
            .unwrap();
        wait_receipt(
            &shell,
            &last.id,
            1,
            alan_agent_protocol::UiInputStatus::Completed,
        )
        .await;
        assert!(
            !drop_receipt || fault.dropped.lock().unwrap().is_none(),
            "lost-receipt case must traverse the actual AgentFS write fault"
        );
        let before = captures.lock().unwrap().clone();
        runtime
            .handle
            .submission_tx
            .send(first.clone())
            .await
            .unwrap();
        wait_receipt(
            &shell,
            &first.id,
            if drop_receipt { 1 } else { 2 },
            expected,
        )
        .await;
        assert_eq!(
            *captures.lock().unwrap(),
            before,
            "repeated selection must re-publish receipt without another capture/install"
        );
        let input = Submission::new(Op::Input {
            parts: vec![ContentPart::text("after delayed retry")],
            mode: InputMode::FollowUp,
        });
        runtime
            .handle
            .submission_tx
            .send(input.clone())
            .await
            .unwrap();
        wait_receipt(
            &shell,
            &input.id,
            1,
            alan_agent_protocol::UiInputStatus::Completed,
        )
        .await;
        assert_eq!(
            c.recorded_requests().len(),
            1,
            "subsequent input must use C"
        );
        let history = crate::rollout::RolloutRecorder::load_history(&path)
            .await
            .unwrap();
        let selections = history
            .iter()
            .filter(|item| {
                matches!(item,
            crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selected_v1"
            && e.payload["submission_id"] == first.id)
            })
            .count();
        assert_eq!(selections, usize::from(!failed));
        assert_eq!(history.iter().filter(|item| matches!(item,
            crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selection_failed_v1"
            && e.payload["submission_id"] == first.id)).count(), usize::from(failed));
        assert!(history.iter().any(|item| matches!(item,
            crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1"
            && e.payload["id"] == input.id && e.payload["callable_binding"]["model"] == "C")));
    })
    .catch_unwind()
    .await;
    runtime.shutdown().await.unwrap();
    if let Err(panic) = observed {
        std::panic::resume_unwind(panic);
    }

    let (mut runtime, shell, captures, _, _) = selection_runtime(stores, Some(path), None);
    runtime.wait_until_ready().await.unwrap();
    let observed = std::panic::AssertUnwindSafe(async {
        let before = captures.lock().unwrap().clone();
        runtime
            .handle
            .submission_tx
            .send(first.clone())
            .await
            .unwrap();
        wait_receipt(&shell, &first.id, 1, expected).await;
        assert_eq!(
            *captures.lock().unwrap(),
            before,
            "restored receipt must not recapture"
        );
        let snapshot: alan_agent_protocol::UiModelSnapshot =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await.unwrap())
                .unwrap();
        assert_eq!(snapshot.selected_next.unwrap().model, "C");
        let history = crate::rollout::RolloutRecorder::load_history(
            runtime
                .wait_until_ready()
                .await
                .unwrap()
                .rollout_path
                .as_ref()
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(history.iter().filter(|item| matches!(item,
            crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selected_v1"
            && e.payload["submission_id"] == first.id)).count(), usize::from(!failed),
            "recovery preserves original event once; redelivery must not add another");
        assert_eq!(history.iter().filter(|item| matches!(item,
            crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_model_selection_failed_v1"
            && e.payload["submission_id"] == first.id)).count(), usize::from(failed));
    })
    .catch_unwind()
    .await;
    runtime.shutdown().await.unwrap();
    if let Err(panic) = observed {
        std::panic::resume_unwind(panic);
    }
}

fn selection_runtime(
    stores: crate::AgentRuntimeStoreBindings,
    recovery: Option<std::path::PathBuf>,
    drop_receipt: Option<String>,
) -> (
    RuntimeController,
    alan_shell::Shell,
    Captures,
    MockLlmProvider,
    Arc<ReceiptFault>,
) {
    let c = MockLlmProvider::new();
    let llmfs = alan_llmfs::LlmFs::new();
    for name in ["A", "B", "C"] {
        llmfs.register_connection(
            name,
            Box::new(if name == "C" {
                c.clone()
            } else {
                MockLlmProvider::new()
            }),
        );
    }
    let mut ns = alan_kernel::Namespace::new();
    let fault = Arc::new(ReceiptFault {
        inner: alan_agentfs::AgentFs::new(),
        dropped: Mutex::new(drop_receipt),
        paths: Default::default(),
    });
    ns.mount(
        "/agent/1",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let captures = Arc::new(Mutex::new(Vec::new()));
    let authority = SelectionCatalog {
        values: ["A", "B", "C"]
            .into_iter()
            .map(|model| CapturedCallable {
                identity: CallableIdentity {
                    profile: "managed".into(),
                    provider: "openai_responses".into(),
                    model: model.into(),
                    credential_ref: None,
                    revision: model.into(),
                },
                root: root.clone(),
                connection: model.into(),
                config: core.clone(),
            })
            .collect(),
        captures: captures.clone(),
    };
    let runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores),
            recovery_rollout_path: recovery,
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "A")
            .with_connection_authority(Arc::new(authority)),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    (runtime, shell, captures, c, fault)
}

async fn wait_receipt(
    shell: &alan_shell::Shell,
    id: &str,
    count: usize,
    expected: alan_agent_protocol::UiInputStatus,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events=String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
            let found=events.lines().filter_map(|line| serde_json::from_str::<alan_agent_protocol::UiEvent>(line).ok())
                .filter(|event| matches!(event, alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,status,..} if submission_ids == &vec![id.to_owned()] && *status == expected)).count();
            if found >= count {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("correlated selection/input receipt must arrive");
}

struct ReceiptFault {
    inner: alan_agentfs::AgentFs,
    dropped: Mutex<Option<String>>,
    paths: Mutex<std::collections::HashSet<u64>>,
}
#[async_trait]
impl alan_ap::FileServer for ReceiptFault {
    async fn walk(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        names: &[String],
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        let qid = self.inner.walk(fid, newfid, names).await?;
        if names.ends_with(&["machine".into(), "ui".into(), "events".into()]) {
            self.paths.lock().unwrap().insert(qid.path);
        }
        Ok(qid)
    }
    async fn open(
        &self,
        fid: alan_ap::Fid,
        mode: alan_ap::OpenMode,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        count: u32,
    ) -> Result<Vec<u8>, alan_ap::ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, alan_ap::ErrorCode> {
        let path = self.inner.stat(fid).await?.qid.path;
        if self.paths.lock().unwrap().contains(&path) {
            let event = serde_json::from_slice::<alan_agent_protocol::UiEvent>(data);
            if let Ok(alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. }) = event {
                let mut dropped = self.dropped.lock().unwrap();
                if dropped
                    .as_ref()
                    .is_some_and(|id| submission_ids.contains(id))
                {
                    *dropped = None;
                    return Err(alan_ap::ErrorCode::Io);
                }
            }
        }
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: alan_ap::Fid) -> Result<alan_ap::Stat, alan_ap::ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        name: &str,
        kind: alan_ap::FileKind,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.clunk(fid).await
    }
}
