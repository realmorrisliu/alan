    use super::*;
    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        runtime::NamespaceRuntimeEnvironment,
        tools::{Tool, ToolContext, ToolRegistry, ToolResult},
    };
    use alan_agent_protocol::ToolCapability;
    use alan_agentfs::AgentFs;
    use alan_ap::{
        ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Request, Stat,
    };
    use alan_kernel::{
        Access, Credentials, MountFs, Namespace, ProcFs, ProcessInvocation, ProcessOutcome,
        ProcessRunner,
    };
    use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
    use alan_shell::Shell;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::Mutex;

    // Simple mock provider for testing
    struct SimpleMockProvider;

    #[async_trait]
    impl LlmProvider for SimpleMockProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Ok(GenerationResponse {
                content: "test".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: vec![],
                usage: None,
                finish_reason: None,
                warnings: Vec::new(),
                provider_response_id: None,
                provider_response_status: None,
            })
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("mock".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(StreamChunk {
                    text: Some("test".to_string()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "mock"
        }
    }

    #[derive(Clone, Copy)]
    enum BinNode {
        Root,
        Tool(&'static str),
    }

    const BUILTIN_BIN_TOOLS: [&str; 7] = [
        "read_file",
        "write_file",
        "edit_file",
        "bash",
        "grep",
        "glob",
        "list_dir",
    ];

    struct StaticBinFs {
        fids: Mutex<HashMap<Fid, BinNode>>,
    }

    impl StaticBinFs {
        fn new() -> Self {
            let mut fids = HashMap::new();
            fids.insert(Fid::ROOT, BinNode::Root);
            Self {
                fids: Mutex::new(fids),
            }
        }
    }

    #[async_trait]
    impl FileServer for StaticBinFs {
        async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
            let mut fids = self.fids.lock().await;
            if newfid == Fid::ROOT || fids.contains_key(&newfid) {
                return Err(ErrorCode::BadRequest);
            }
            let mut node = *fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            for name in names {
                node = match (node, name.as_str()) {
                    (BinNode::Root, name) => BUILTIN_BIN_TOOLS
                        .iter()
                        .copied()
                        .find(|tool| *tool == name)
                        .map(BinNode::Tool)
                        .ok_or(ErrorCode::NotFound)?,
                    (BinNode::Tool(_), _) => return Err(ErrorCode::NotDirectory),
                };
            }
            fids.insert(newfid, node);
            Ok(bin_qid(node))
        }

        async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
            let fids = self.fids.lock().await;
            let node = *fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(mode, OpenMode::Read) {
                return Err(ErrorCode::NoAccess);
            }
            Ok(bin_qid(node))
        }

        async fn read(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>, ErrorCode> {
            let fids = self.fids.lock().await;
            let node = *fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            let bytes = match node {
                BinNode::Root => BUILTIN_BIN_TOOLS.join("\n").into_bytes(),
                BinNode::Tool(_) => Vec::new(),
            };
            let start = (offset as usize).min(bytes.len());
            let end = bytes.len().min(start + count as usize);
            Ok(bytes[start..end].to_vec())
        }

        async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
            let fids = self.fids.lock().await;
            let node = *fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            Ok(Stat {
                name: String::new(),
                qid: bin_qid(node),
                length: match node {
                    BinNode::Root => BUILTIN_BIN_TOOLS.join("\n").len() as u64,
                    BinNode::Tool(_) => 0,
                },
                executable: false,
                writable: false,
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

        async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
            if fid != Fid::ROOT {
                self.fids.lock().await.remove(&fid);
            }
            Ok(())
        }
    }

    fn bin_qid(node: BinNode) -> Qid {
        match node {
            BinNode::Root => Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 1,
            },
            BinNode::Tool(name) => Qid {
                kind: FileKind::File,
                version: 0,
                path: 2 + BUILTIN_BIN_TOOLS
                    .iter()
                    .position(|tool| *tool == name)
                    .unwrap_or(0) as u64,
            },
        }
    }

    struct JsonToolRunner;

    #[async_trait]
    impl ProcessRunner for JsonToolRunner {
        async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
            let Ok(resolved) = invocation.namespace.resolve(&invocation.exec.executable) else {
                return ProcessOutcome::exited(
                    127,
                    br#"{"success":false,"error":"executable is not mounted"}"#.to_vec(),
                );
            };
            let fid = Fid(70_000 + invocation.pid.0);
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
                return ProcessOutcome::exited(
                    127,
                    br#"{"success":false,"error":"executable is not reachable"}"#.to_vec(),
                );
            }
            let arguments = invocation
                .exec
                .args
                .first()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or(Value::Null);
            let tool_name = invocation
                .exec
                .executable
                .rsplit('/')
                .next()
                .unwrap_or(invocation.exec.executable.as_str());
            let mut output = json!({
                "success": true,
                "tool": tool_name,
                "content": format!("from namespace {tool_name}"),
                "arguments": arguments,
            })
            .to_string()
            .into_bytes();
            output.push(b'\n');
            ProcessOutcome::exited(0, output)
        }
    }

    struct RegistryToolRunner {
        tools: ToolRegistry,
    }

    impl RegistryToolRunner {
        fn new(tools: ToolRegistry) -> Self {
            Self { tools }
        }
    }

    #[async_trait]
    impl ProcessRunner for RegistryToolRunner {
        async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
            if invocation
                .namespace
                .resolve(&invocation.exec.executable)
                .is_err()
            {
                return ProcessOutcome::exited(127, b"executable is not mounted\n".to_vec());
            }
            let tool_name = invocation
                .exec
                .executable
                .rsplit('/')
                .next()
                .unwrap_or(invocation.exec.executable.as_str());
            let arguments = invocation
                .exec
                .args
                .first()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .unwrap_or(Value::Null);

            match self.tools.execute(tool_name, arguments).await {
                Ok(output) => {
                    let mut bytes = serde_json::to_vec(&output)
                        .unwrap_or_else(|_| b"{\"success\":true}".to_vec());
                    bytes.push(b'\n');
                    ProcessOutcome::exited(0, bytes)
                }
                Err(err) => {
                    let mut bytes = serde_json::to_vec(&json!({
                        "success": false,
                        "error": format!("{err:#}"),
                    }))
                    .unwrap_or_else(|_| b"{\"success\":false}".to_vec());
                    bytes.push(b'\n');
                    ProcessOutcome::exited(1, bytes)
                }
            }
        }
    }

    struct CountingEffectTool {
        name: &'static str,
        capability: ToolCapability,
        counter: Arc<AtomicUsize>,
    }

    impl Tool for CountingEffectTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "Counting side-effect tool used for dedupe tests"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "payload": {"type": "string"}
                }
            })
        }

        fn execute(&self, arguments: Value, _ctx: &ToolContext) -> ToolResult {
            let counter = Arc::clone(&self.counter);
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(json!({
                    "ok": true,
                    "payload": arguments
                }))
            })
        }

        fn capability(&self, _arguments: &Value) -> ToolCapability {
            self.capability
        }
    }

    fn create_test_state() -> RuntimeLoopState {
        let config = Config::default();
        let machine = AgentMachine::new();
        let runtime_config = crate::runtime::RuntimeConfig::default();
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(AgentFs::new())),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));

        RuntimeLoopState {
            machine,
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: config,
            runtime_config,
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        }
    }

    fn create_namespace_test_state_and_shell() -> (RuntimeLoopState, Shell) {
        create_namespace_test_state_and_shell_with_bin(true)
    }

    fn create_namespace_test_state_and_shell_with_bin(
        mount_bin: bool,
    ) -> (RuntimeLoopState, Shell) {
        create_namespace_test_state_and_shell_with_package(mount_bin, mount_bin)
    }

    fn create_namespace_test_state_and_shell_with_package(
        mount_bin: bool,
        mount_manifests: bool,
    ) -> (RuntimeLoopState, Shell) {
        let procfs = ProcFs::new().with_runner(Arc::new(JsonToolRunner));
        let agentfs = Arc::new(AgentFs::new());
        let bin = InProcessTransport::new(Arc::new(StaticBinFs::new()));
        let mut manifests = Vec::new();

        let mut child_namespace = Namespace::new();
        child_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs.clone()),
            Access::ReadWrite,
        );
        if mount_bin {
            child_namespace.mount("/bin", bin.clone(), Access::ReadOnly);
        }
        if mount_manifests {
            for tool_name in BUILTIN_BIN_TOOLS {
                let manifest = crate::runtime::ToolPackageManifest {
                    version: 1,
                    name: tool_name.to_string(),
                    description: format!("Test {tool_name} Tool"),
                    parameters: json!({"type": "object"}),
                    capability: ToolCapability::Read,
                    capability_is_argument_dependent: false,
                    timeout_secs: 30,
                    execution: crate::runtime::tool_packages::ToolExecutionHints {
                        arguments: "json_first_arg".to_string(),
                        result: "stdout_json".to_string(),
                    },
                };
                let path = format!("/lib/exec/{tool_name}");
                let transport = InProcessTransport::new(Arc::new(
                    alan_ap::reference::MemFs::with_read_only_file(
                        "manifest",
                        serde_json::to_vec(&manifest).unwrap(),
                    ),
                ));
                child_namespace.mount(&path, transport.clone(), Access::ReadOnly);
                manifests.push((path, transport));
            }
        }

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
        if mount_bin {
            root_namespace.mount("/bin", bin, Access::ReadOnly);
        }
        for (path, transport) in manifests {
            root_namespace.mount(&path, transport, Access::ReadOnly);
        }
        let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
        let shell = Shell::new(root.clone());
        let state = RuntimeLoopState {
            machine: AgentMachine::new(),
            environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
            core_config: Config::default(),
            runtime_config: crate::runtime::RuntimeConfig::default(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        };
        (state, shell)
    }

    fn reviewer_response(decision: &str) -> alan_llm::GenerationResponse {
        alan_llm::GenerationResponse {
            content: format!("{{\"decision\":\"{decision}\",\"rationale\":\"test\"}}"),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }
    }

    fn escalating_state_with_reviewer(
        counter: &Arc<AtomicUsize>,
        decision: &str,
    ) -> RuntimeLoopState {
        let mut machine = AgentMachine::new();
        machine.add_user_message("do the thing");
        let mut tools = ToolRegistry::new();
        tools.register(CountingEffectTool {
            // Unknown capability → autonomous policy escalates → reviewer route.
            name: "do_thing",
            capability: ToolCapability::Unknown,
            counter: Arc::clone(counter),
        });
        create_test_state_with_machine_tools_and_provider(
            machine,
            tools,
            alan_llm::MockLlmProvider::new().with_response(reviewer_response(decision)),
        )
    }

    #[tokio::test]
    async fn reviewer_allow_executes_escalated_tool() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut state = escalating_state_with_reviewer(&counter, "allow");
        let _ = execute_single_tool_call(&mut state, "c1", "do_thing", json!({})).await;
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "reviewer-approved escalation should execute"
        );
    }

    #[tokio::test]
    async fn reviewer_deny_blocks_escalated_tool_and_feeds_back() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut state = escalating_state_with_reviewer(&counter, "deny");
        let (_, events) = execute_single_tool_call(&mut state, "c1", "do_thing", json!({})).await;
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "reviewer-denied escalation must not execute"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Warning { message } if message.contains("auto-review denied")
        )));
    }

    #[tokio::test]
    async fn reviewer_repeated_denials_trip_breaker_to_human() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut state = escalating_state_with_reviewer(&counter, "deny");
        // Third consecutive denial trips the circuit breaker and pauses for a human.
        execute_single_tool_call(&mut state, "c1", "do_thing", json!({})).await;
        execute_single_tool_call(&mut state, "c2", "do_thing", json!({})).await;
        let (_, events) = execute_single_tool_call(&mut state, "c3", "do_thing", json!({})).await;
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Yield {
                kind: alan_agent_protocol::YieldKind::Confirmation,
                ..
            }
        )));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    fn create_test_state_with_machine_and_tools(
        machine: AgentMachine,
        tools: ToolRegistry,
    ) -> RuntimeLoopState {
        create_test_state_with_machine_tools_and_provider(machine, tools, SimpleMockProvider)
    }

    fn create_test_state_with_machine_tools_and_provider<P: LlmProvider + 'static>(
        machine: AgentMachine,
        tools: ToolRegistry,
        provider: P,
    ) -> RuntimeLoopState {
        create_test_state_with_machine_tools_provider_and_agent_path(
            machine, tools, provider, "/agent/1",
        )
    }

    fn create_test_state_with_machine_tools_provider_and_agent_path<P: LlmProvider + 'static>(
        machine: AgentMachine,
        mut tools: ToolRegistry,
        provider: P,
        agent_path: &str,
    ) -> RuntimeLoopState {
        let agentfs = Arc::new(AgentFs::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(provider));

        let mut process_namespace = Namespace::new();
        process_namespace.mount(
            agent_path,
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        process_namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        for tool_name in tools.list_tools() {
            process_namespace.mount(
                &format!("/bin/{tool_name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
                Access::ReadOnly,
            );
            let tool = tools.get(tool_name).unwrap();
            let manifest = crate::runtime::ToolPackageManifest::from_tool(
                tool.as_ref(),
                tools.execution_timeout_secs(tool_name).unwrap_or(30),
            )
            .unwrap();
            process_namespace.mount(
                &format!("/lib/exec/{tool_name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(&manifest).unwrap(),
                ))),
                Access::ReadOnly,
            );
        }

        let launch_context = crate::ProcessLaunchContext::new(
            process_namespace.clone(),
            Credentials::user("test-agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", "/tmp", Access::ReadWrite).unwrap(),
        );
        let binding = crate::tools::ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/alan-agent-engine-test-scratch"),
        )
        .unwrap();
        tools.set_default_execution_binding(binding.clone());

        let procfs = ProcFs::new().with_runner(Arc::new(RegistryToolRunner::new(tools.clone())));
        let tool_runner = crate::tools::ToolProcessRunner::from_registry(&tools);
        tool_runner.register_process_binding(alan_kernel::Pid(1), binding);
        let spawner_procfs = Arc::new(procfs.for_spawner(
            None,
            process_namespace.clone(),
            Credentials::user("root-agent"),
        ));
        process_namespace.mount(
            "/proc",
            InProcessTransport::new(spawner_procfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(process_namespace)));
        let config = Config {
            openai_responses_model: "mock-model".to_string(),
            ..Default::default()
        };
        RuntimeLoopState {
            machine,
            environment: NamespaceRuntimeEnvironment::new(root, agent_path, "default")
                .with_launch_context(launch_context)
                .with_tool_process_context(alan_kernel::Pid(1), tool_runner),
            core_config: config,
            runtime_config: crate::runtime::RuntimeConfig::default(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
        }
    }

    async fn execute_single_tool_call(
        state: &mut RuntimeLoopState,
        call_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> (ToolBatchOrchestratorOutcome, Vec<Event>) {
        let mut loop_guard = ToolLoopGuard::new(None, 4);
        let cancel = CancellationToken::new();
        let tool_calls = vec![NormalizedToolCall {
            id: call_id.to_string(),
            name: tool_name.to_string(),
            arguments,
        }];
        let inputs = ToolOrchestratorInputs {
            cancel: &cancel,
            steering_broker: None,
        };

        let mut events = Vec::new();
        let mut emit = |event: Event| {
            events.push(event);
            async {}
        };

        let outcome = orchestrate_tool_batch(
            &mut loop_guard,
            state,
            &tool_calls,
            inputs,
            &mut emit,
        )
        .await
        .expect("tool orchestration should succeed");
        (outcome, events)
    }

    async fn read_shell_utf8(shell: &Shell, path: &str) -> String {
        String::from_utf8(shell.cat(path).await.expect("read agent file")).expect("agent file utf8")
    }
