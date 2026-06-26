//! `FileLlmProvider` — an [`LlmProvider`] that reads its generations from llmfs
//! files over aP.
//!
//! This is the engine-rewiring seam of the Plan 9 program: the agent engine
//! keeps calling the same `LlmProvider` trait, but a generation now does
//! clone-via-open on `connections/<conn>/clone`, writes the request document to
//! `data`, and reads the token stream from `events` — so *the agent reads its
//! LLM as a file* (ADR-0024) with **no change to the engine loop**. Swapping the
//! engine onto this provider is then configuration, not a rewrite.
//!
//! It is a client of llmfs (talks aP via [`InProcessTransport`]); it depends on
//! `alan-ap` + `alan-llm` (the trait it implements), never on the `alan-llmfs`
//! crate.

use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, MessageRole, StreamChunk};
use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// An `LlmProvider` backed by an llmfs connection reached over aP.
pub struct FileLlmProvider {
    fs: InProcessTransport,
    connection: String,
    next_fid: AtomicU64,
}

impl FileLlmProvider {
    /// Build a provider over an llmfs transport, bound to one connection.
    pub fn new(fs: InProcessTransport, connection: &str) -> Self {
        // Fid 0 is the well-known root; client fids start above it.
        Self {
            fs,
            connection: connection.to_string(),
            next_fid: AtomicU64::new(1),
        }
    }

    fn alloc_fid(&self) -> Fid {
        Fid(self.next_fid.fetch_add(1, Ordering::Relaxed))
    }

    async fn call(&self, request: Request) -> Result<Response> {
        self.fs
            .call(request)
            .await
            .map_err(|e| anyhow!("aP error: {e:?}"))
    }

    /// Start a generation: clone-via-open to allocate it, then commit the request
    /// document on `data`. Returns the generation id.
    async fn start_generation(&self, request: &GenerationRequest) -> Result<String> {
        let conn = self.connection.clone();

        // clone-via-open → the new generation's id.
        let clone_fid = self.alloc_fid();
        self.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: clone_fid,
            names: vec!["connections".into(), conn.clone(), "clone".into()],
        })
        .await?;
        self.call(Request::Open {
            fid: clone_fid,
            mode: OpenMode::ReadWrite,
        })
        .await?;
        let gen_id = match self
            .call(Request::Read {
                fid: clone_fid,
                offset: 0,
                count: 64,
            })
            .await?
        {
            Response::Read { data } => String::from_utf8(data)?,
            _ => bail!("clone read did not return the generation id"),
        };

        // Map the neutral request and commit it on `data` (commit-on-clunk).
        let user = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let doc = serde_json::json!({ "system": request.system_prompt, "user": user }).to_string();

        let data_fid = self.alloc_fid();
        self.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: data_fid,
            names: vec!["connections".into(), conn, gen_id.clone(), "data".into()],
        })
        .await?;
        self.call(Request::Open {
            fid: data_fid,
            mode: OpenMode::Write,
        })
        .await?;
        self.call(Request::Write {
            fid: data_fid,
            offset: 0,
            data: doc.into_bytes(),
        })
        .await?;
        self.call(Request::Clunk { fid: data_fid }).await?;

        Ok(gen_id)
    }
}

#[async_trait]
impl LlmProvider for FileLlmProvider {
    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        let gen_id = self.start_generation(&request).await?;

        // Open the events stream and forward its records as StreamChunks.
        let events_fid = self.alloc_fid();
        self.call(Request::Walk {
            fid: Fid::ROOT,
            newfid: events_fid,
            names: vec![
                "connections".into(),
                self.connection.clone(),
                gen_id,
                "events".into(),
            ],
        })
        .await?;
        self.call(Request::Open {
            fid: events_fid,
            mode: OpenMode::Read,
        })
        .await?;

        let (tx, rx) = mpsc::channel(64);
        let fs = self.fs.clone();
        tokio::spawn(async move {
            let mut offset = 0u64;
            let mut buf = String::new();
            loop {
                let data = match fs
                    .call(Request::Read {
                        fid: events_fid,
                        offset,
                        count: 65536,
                    })
                    .await
                {
                    Ok(Response::Read { data }) if !data.is_empty() => data,
                    _ => break, // stream closed or errored
                };
                offset += data.len() as u64;
                buf.push_str(&String::from_utf8_lossy(&data));

                while let Some(nl) = buf.find('\n') {
                    let line: String = buf.drain(..=nl).collect();
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                        continue;
                    };
                    if value.get("done").is_some() {
                        let _ = tx.send(finished_chunk()).await;
                        return;
                    }
                    if let Some(text) = value.get("text").and_then(|t| t.as_str())
                        && tx.send(text_chunk(text)).await.is_err()
                    {
                        return;
                    }
                }
            }
        });
        Ok(rx)
    }

    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        let mut rx = self.generate_stream(request).await?;
        let mut content = String::new();
        while let Some(chunk) = rx.recv().await {
            if let Some(text) = chunk.text {
                content.push_str(&text);
            }
            if chunk.is_finished {
                break;
            }
        }
        Ok(GenerationResponse {
            content,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        let mut request = GenerationRequest::new().with_user_message(user);
        if let Some(system) = system {
            request = request.with_system_prompt(system);
        }
        Ok(self.generate(request).await?.content)
    }

    fn provider_name(&self) -> &'static str {
        "llmfs"
    }
}

fn text_chunk(text: &str) -> StreamChunk {
    StreamChunk {
        text: Some(text.to_string()),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        provider_response_id: None,
        provider_response_status: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished: false,
        finish_reason: None,
    }
}

fn finished_chunk() -> StreamChunk {
    StreamChunk {
        is_finished: true,
        ..text_chunk("")
    }
}
