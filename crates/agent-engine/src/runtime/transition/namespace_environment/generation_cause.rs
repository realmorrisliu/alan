use alan_ap::{ErrorCode, OpenMode};
use anyhow::Result;

use super::{LlmEvent, NamespaceClient};

const MAX_CAUSE_BYTES: u64 = 4096;
const CAUSE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) fn safe_cause(reason: &str) -> &'static str {
    let reason = alan_llm::safe_finish_reason(reason);
    if reason.starts_with("stream_error:") || reason == "stream_error" {
        reason
    } else {
        "stream_error:unknown"
    }
}

fn published_cause(bytes: &[u8]) -> &'static str {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return "stream_error:unknown";
    }
    for line in bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let Ok(event) = serde_json::from_slice::<LlmEvent>(line) else {
            return "stream_error:unknown";
        };
        if event.version.is_some_and(|version| version != 1) {
            return "stream_error:unknown";
        }
        if let Some(error) = event.error {
            return safe_cause(&error);
        }
    }
    "stream_error:unknown"
}

// Failed commit already owns a generation. Snapshot only its published bytes;
// never tail the live edge, collect a response, or expose a namespace error.
async fn read_published_cause(
    client: &NamespaceClient,
    connection: &str,
    generation: &str,
) -> Result<&'static str> {
    let path = format!("/mnt/llm/connections/{connection}/{generation}/events");
    let fid = client.open_path_guarded(&path, OpenMode::Read).await?;
    let length = client.stat(fid.fid()).await?.length;
    if length == 0 || length > MAX_CAUSE_BYTES {
        fid.close().await?;
        return Ok("stream_error:unknown");
    }
    let mut bytes = Vec::new();
    while (bytes.len() as u64) < length {
        let chunk = client
            .read_at(
                fid.fid(),
                bytes.len() as u64,
                (length - bytes.len() as u64) as u32,
            )
            .await?;
        if chunk.is_empty() || chunk.len() as u64 > length - bytes.len() as u64 {
            return Ok("stream_error:unknown");
        }
        bytes.extend_from_slice(&chunk);
    }
    fid.close().await?;
    Ok(published_cause(&bytes))
}

pub(super) async fn commit_generation(
    client: &NamespaceClient,
    connection: &str,
    generation: &str,
    request: &[u8],
) -> Result<()> {
    let path = format!("/mnt/llm/connections/{connection}/{generation}/data");
    if let Err(error) = client.write_document(&path, request).await {
        let code = error
            .downcast_ref::<ErrorCode>()
            .copied()
            .unwrap_or(ErrorCode::Io);
        let cause = tokio::time::timeout(
            CAUSE_TIMEOUT,
            read_published_cause(client, connection, generation),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or("stream_error:unknown");
        return Err(anyhow::Error::new(code).context(crate::retry::GenerationCause::new(cause)));
    }
    Ok(())
}

#[cfg(test)]
#[path = "generation_cause_tests.rs"]
mod tests;
