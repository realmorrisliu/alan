//! Bounded read-only acquisition of the receiving Process's retained Action evidence.
use alan_ap::{FileKind, InProcessTransport};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use super::client::NamespaceClient;
use crate::evidence::{MAX_EVIDENCE_READ_BYTES, RETENTION_EXPIRED_RECORD_TYPE};

pub(crate) async fn read_action_evidence(
    namespace: InProcessTransport,
    owner: u64,
    path: &str,
    offset: u64,
    length: u64,
) -> Result<Value> {
    let parts = path.split('/').collect::<Vec<_>>();
    ensure!(
        owner > 0
            && parts.len() == 6
            && parts[0].is_empty()
            && parts[1] == "agent"
            && parts[2] == owner.to_string()
            && parts[3] == "actions"
            && !parts[4].is_empty()
            && parts[4].len() <= 64
            && parts[4]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            && parts[5] == "output",
        "Only the receiving Agent's concrete Action output path can be read"
    );
    ensure!(
        (1..=MAX_EVIDENCE_READ_BYTES).contains(&length),
        "Evidence byte limit must be between 1 and {MAX_EVIDENCE_READ_BYTES}"
    );
    let client = NamespaceClient::new(namespace);
    let stat = client.stat_path(path).await?;
    ensure!(stat.qid.kind == FileKind::File, "Evidence is not a file");
    // Expiry replaces the original with a small record; an old offset must not hide it.
    if stat.length <= MAX_EVIDENCE_READ_BYTES {
        let bytes = client.read_file_range(path, 0, stat.length).await?;
        if let Ok(record) = serde_json::from_slice::<Value>(&bytes)
            && record["type"] == RETENTION_EXPIRED_RECORD_TYPE
        {
            return Ok(record);
        }
    }
    ensure!(
        offset <= stat.length,
        "Evidence byte offset is past the retained end"
    );
    let bytes = client
        .read_file_range(path, offset, length.min(stat.length - offset))
        .await?;
    let content = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()])?
        }
        Err(error) => return Err(error).context("Evidence range must start at a UTF-8 boundary"),
    };
    let end = offset + content.len() as u64;
    ensure!(
        end > offset || offset == stat.length,
        "Evidence byte limit cannot fit the next UTF-8 character"
    );
    Ok(json!({
        "type": "text", "path": path, "content": content,
        "byte_offset": offset, "next_byte_offset": end,
        "total_bytes": stat.length, "truncated": end < stat.length,
    }))
}
