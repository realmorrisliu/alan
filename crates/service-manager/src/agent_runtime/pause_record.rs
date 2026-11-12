//! Pause record captured while a child Agent Process is still observable.

use alan_agent_engine::{AgentExecutablePause, YieldKind};
use alan_kernel::Pid;
use anyhow::{Context, Result};

pub(super) async fn read_child_pause(
    shell: &alan_shell::Shell,
    pid: Pid,
) -> Result<Option<AgentExecutablePause>> {
    let requests_path = format!("/agent/{}/requests", pid.0);
    for request_id in shell.ls(&requests_path).await? {
        if matches!(request_id.as_str(), "clone" | "events") {
            continue;
        }
        let request_path = format!("{requests_path}/{request_id}");
        if shell.cat(&format!("{request_path}/status")).await? != b"pending" {
            continue;
        }
        let kind = String::from_utf8(shell.cat(&format!("{request_path}/kind")).await?)
            .context("child Agent request kind is utf8")?;
        let kind = match kind.as_str() {
            "confirmation" => YieldKind::Confirmation,
            "structured_input" => YieldKind::StructuredInput,
            other => YieldKind::Custom(other.to_string()),
        };
        // Capture the decision details now: AgentFS is the live view and is
        // removed once this Process exits, so a consumer that reads later would
        // otherwise lose an inherited stop's ancestor identity.
        let payload = shell
            .cat(&format!("{request_path}/options"))
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|options| options.get("details").cloned());
        return Ok(Some(AgentExecutablePause {
            request_id,
            kind,
            payload,
        }));
    }
    Ok(None)
}
