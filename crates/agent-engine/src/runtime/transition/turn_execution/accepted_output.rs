//! Publication of an accepted draft's remaining output once its generation has settled.

use alan_agent_protocol::Event;
use anyhow::{Context, Result};

use super::live_stream::LiveTurnStream;
use crate::runtime::transition::NamespaceAgentFiles;
use crate::runtime::turn_support::{emit_streaming_chunks, emit_thinking_chunks};

/// Release held text, then publish whatever the live stream did not already cover.
///
/// Returns whether the assistant text reached `io/output` live, so the caller does not write
/// the settled message a second time.
pub(super) async fn publish_accepted<E, F>(
    live: &LiveTurnStream,
    files: &NamespaceAgentFiles,
    content: &str,
    thinking: Option<&str>,
    emit: &mut E,
) -> Result<bool>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    live.settle(live.plan_accept(), emit).await?;
    let streamed = live.text_visible();
    if let Some(rest) = live.unstreamed_remainder(content) {
        files
            .write_assistant_output(&rest)
            .await
            .context("write namespace assistant output")?;
    }
    if let Some(thinking) = thinking
        && !thinking.is_empty()
        && !live.thinking_published()
    {
        crate::runtime::ui_surfaces::thinking(files, thinking)
            .await
            .context("write thinking UI state")?;
        emit_thinking_chunks(emit, thinking).await;
    }
    if !content.is_empty() {
        if streamed {
            emit(Event::TextDelta {
                chunk: String::new(),
                is_final: true,
            })
            .await;
        } else {
            emit_streaming_chunks(emit, content).await;
        }
    }
    Ok(streamed)
}

/// A generation failed or was cancelled: close reasoning left open and drop held text.
pub(super) async fn abandon<E, F>(live: &LiveTurnStream, emit: &mut E)
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let _ = live.settle(live.plan_discard(), emit).await;
}
