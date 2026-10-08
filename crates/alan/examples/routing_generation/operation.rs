//! Shared advice-only generation operation for component and native baselines.
use alan_agent_engine::GenerationRequest;
use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct Attempt {
    pub(super) allocator: Option<Fid>,
    pub(super) operation: Option<String>,
    pub(super) tail: Option<alan_shell::Tail>,
    pub(super) raw: Vec<u8>,
    pub(super) events: Vec<Value>,
    pub(super) text: String,
    pub(super) status: Value,
}

pub(super) fn terminal(event: &Value) -> bool {
    event["version"] == 1
        && [
            event["done"] == true,
            event["aborted"] == true,
            event["rejected"] == true,
            event.get("error").is_some(),
        ]
        .into_iter()
        .filter(|flag| *flag)
        .count()
            == 1
}

pub(super) async fn generate(
    root: &InProcessTransport,
    shell: &alan_shell::Shell,
    connection: &str,
    body: &Value,
    index: u64,
    attempt: &mut Attempt,
    path: &std::path::Path,
) -> Result<()> {
    let fid = Fid(1_000_000 + index);
    attempt.allocator = Some(fid);
    root.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: fid,
        names: format!("{connection}/clone")
            .split('/')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
    })
    .await?;
    root.call(Request::Open {
        fid,
        mode: OpenMode::ReadWrite,
    })
    .await?;
    let Response::Read { data } = root
        .call(Request::Read {
            fid,
            offset: 0,
            count: 128,
        })
        .await?
    else {
        anyhow::bail!("invalid allocator response")
    };
    let id = std::str::from_utf8(&data)?.trim();
    ensure!(
        !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid generation id"
    );
    let operation = format!("{connection}/{id}");
    attempt.operation = Some(operation.clone());
    // Persist the allocated identity before any paid commit can begin.
    let mut receipt: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    receipt["operation_path"] = json!(operation);
    std::fs::write(path, serde_json::to_vec_pretty(&receipt)?)?;
    attempt.allocator = None;
    root.call(Request::Clunk { fid }).await?;
    shell
        .write(&format!("{operation}/data"), &serde_json::to_vec(body)?)
        .await?;
    attempt.tail = Some(shell.tail(&format!("{operation}/events")).await?);
    let mut receipt: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    receipt["events_tail_opened"] = json!(true);
    std::fs::write(path, serde_json::to_vec_pretty(&receipt)?)?;
    let mut parsed = 0;
    loop {
        let bytes = attempt.tail.as_mut().unwrap().read(4096).await?;
        ensure!(
            !bytes.is_empty() && attempt.raw.len() + bytes.len() <= 1 << 20,
            "invalid event stream"
        );
        attempt.raw.extend(bytes);
        while let Some(end) = attempt.raw[parsed..].iter().position(|b| *b == b'\n') {
            let event: Value = serde_json::from_slice(&attempt.raw[parsed..parsed + end])?;
            parsed += end + 1;
            if let Some(delta) = event["text"].as_str() {
                attempt.text.push_str(delta);
            }
            let ended = terminal(&event);
            attempt.events.push(event);
            if ended {
                // Event publication precedes status advancement; retain the caller's deadline.
                loop {
                    attempt.status =
                        serde_json::from_slice(&shell.cat(&format!("{operation}/status")).await?)?;
                    if matches!(
                        attempt.status["status"].as_str(),
                        Some("done" | "error" | "rejected" | "aborted")
                    ) {
                        return Ok(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        }
    }
}

pub(super) fn classify(text: &str, events: &Value, status: &Value) -> &'static str {
    let Some(events) = events.as_array() else {
        return "malformed";
    };
    if status["status"] != "done" {
        return "unavailable";
    }
    if events.iter().any(|e| {
        e["version"] != 1
            || e.get("tool_call").is_some()
            || e.get("error").is_some()
            || e.get("aborted").is_some()
            || e.get("rejected").is_some()
    }) || events.last().is_none_or(|e| e["done"] != true)
        || !matches!(text.trim(), "command" | "agent" | "ambiguous" | "none")
    {
        return "malformed";
    }
    "success"
}

#[cfg(test)]
#[path = "operation/tests.rs"]
mod tests;

pub(super) fn advice_body(input: &str, instructions: &str) -> Value {
    let request = GenerationRequest::new().with_user_message(input);
    json!({"version":2,"system":instructions,"messages":request.messages,
        "tools":[],"reasoning":{"effort":"medium"}})
}

pub(super) async fn settle_attempt(
    root: &InProcessTransport,
    shell: &alan_shell::Shell,
    attempt: &mut Attempt,
) -> Value {
    use std::time::Duration;
    let mut receipt = json!({});
    if !attempt.events.last().is_some_and(terminal) {
        if let Some(operation) = &attempt.operation {
            let aborted = tokio::time::timeout(
                Duration::from_secs(1),
                shell.write(&format!("{operation}/ctl"), b"abort"),
            )
            .await;
            receipt["abort"] = json!(if matches!(aborted, Ok(Ok(()))) {
                "acknowledged"
            } else {
                "uncertain"
            });
        }
    } else {
        receipt["abort"] = json!("already_terminal");
    }
    if let Some(tail) = attempt.tail.take() {
        receipt["tail_closed"] = json!(matches!(
            tokio::time::timeout(Duration::from_secs(1), tail.close()).await,
            Ok(Ok(()))
        ));
    }
    if let Some(fid) = attempt.allocator.take() {
        receipt["allocator_closed"] = json!(matches!(
            tokio::time::timeout(Duration::from_secs(1), root.call(Request::Clunk { fid })).await,
            Ok(Ok(_))
        ));
    }
    if let Some(operation) = &attempt.operation {
        let status = tokio::time::timeout(
            Duration::from_secs(1),
            shell.cat(&format!("{operation}/status")),
        )
        .await;
        receipt["operation_status_after_cleanup"] = match status {
            Ok(Ok(bytes)) => serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            _ => Value::Null,
        };
    }
    receipt
}
