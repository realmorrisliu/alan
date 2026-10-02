use super::*;

struct StartupFailureProvider {
    parse: bool,
}

#[async_trait::async_trait]
impl LlmProvider for StartupFailureProvider {
    async fn generate(&mut self, _: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        anyhow::bail!("not used")
    }
    async fn chat(&mut self, _: Option<&str>, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("not used")
    }
    async fn generate_stream(
        &mut self,
        _: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        if self.parse {
            let error = serde_json::from_str::<serde_json::Value>("secretbodymarker").unwrap_err();
            Err(anyhow::Error::new(error).context("secretbodymarker https://private.example"))
        } else {
            anyhow::bail!("secretbodymarker https://private.example")
        }
    }
    fn provider_name(&self) -> &'static str {
        "startup_failure"
    }
}

#[tokio::test]
async fn failed_commit_preserves_safe_cause_and_io_identity_through_real_mountfs() {
    for parse in [false, true] {
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection("default", Box::new(StartupFailureProvider { parse }));
        let mut ns = Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");
        let request = GenerationRequest::new().with_user_message("hello");
        for text_events in [false, true] {
            let cancel = CancellationToken::new();
            let error = if text_events {
                let mut ignore = |_event: alan_agent_protocol::Event| async {};
                environment
                    .generation()
                    .generate_with_text_events_controlled(&request, &mut ignore, 1, &cancel)
                    .await
                    .unwrap_err()
            } else {
                environment
                    .generation()
                    .generate_controlled(&request, 1, &cancel)
                    .await
                    .unwrap_err()
            };
            let reason = if parse {
                "stream_error:parse"
            } else {
                "stream_error:unknown"
            };
            assert_eq!(
                error.to_string(),
                format!("llmfs generation failed: {reason}")
            );
            assert_eq!(error.downcast_ref::<ErrorCode>(), Some(&ErrorCode::Io));
            let display = format!("{error:#}");
            assert!(!display.contains("secretbodymarker"));
            assert!(!display.contains("private.example"));
        }
        let shell = Shell::new(root);
        for id in ["g0", "g1"] {
            let bytes = shell
                .cat(&format!("/mnt/llm/connections/default/{id}/status"))
                .await
                .unwrap();
            let status: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(status["status"], "error");
        }
    }
}
