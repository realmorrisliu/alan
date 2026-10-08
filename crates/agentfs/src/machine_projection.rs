//! Runtime-owned acknowledged Machine projections and request restoration.
use super::{AgentFs, ErrorCode, MAX_DOC_BYTES, Node, Request};

impl AgentFs {
    /// Publish the Machine's latest acknowledged evaluation observation.
    ///
    /// Only the owning runtime calls this method after its durability barrier.
    /// AgentFS stores a bounded projection; it does not validate advice or own
    /// recovery history. Public aP clients cannot write this file.
    pub async fn publish_evaluation_observation(
        &self,
        observation: Option<serde_json::Value>,
    ) -> Result<(), ErrorCode> {
        if observation.as_ref().is_some_and(|value| !value.is_object()) {
            return Err(ErrorCode::BadRequest);
        }
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "observation": observation,
        }))
        .map_err(|_| ErrorCode::BadRequest)?;
        if bytes.len() > MAX_DOC_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if state.evaluation != bytes {
            state.evaluation = bytes;
            state.bump(&Node::Evaluation);
            state.events.append(b"evaluation\n").await;
        }
        Ok(())
    }

    /// Restore a runtime-owned durable pending request before publishing recovered work.
    /// This in-process path never accepts an external response or rewrites a live request.
    pub async fn restore_pending_request(
        &self,
        id: &str,
        kind: &str,
        prompt: &str,
        options: &str,
    ) -> Result<(), ErrorCode> {
        let index = id
            .strip_prefix('r')
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n < u64::MAX)
            .ok_or(ErrorCode::BadRequest)?;
        if id != format!("r{index}")
            || kind.is_empty()
            || [kind, prompt, options]
                .iter()
                .any(|v| v.len() > MAX_DOC_BYTES)
        {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if let Some(existing) = state.requests.get(id) {
            if existing.kind != kind || existing.prompt != prompt || existing.options != options {
                return Err(ErrorCode::BadRequest);
            }
            return Ok(());
        }
        state.next_request = state.next_request.max(index + 1);
        state.requests.insert(
            id.into(),
            Request {
                kind: kind.into(),
                prompt: prompt.into(),
                options: options.into(),
                status: "pending".into(),
                response: String::new(),
            },
        );
        state.bump(&Node::RequestsDir);
        state
            .request_events
            .append(format!("created:{id}\n").as_bytes())
            .await;
        state
            .events
            .append(format!("request:{id}\n").as_bytes())
            .await;
        Ok(())
    }

    /// Publish acknowledged Machine work; public clients cannot alter this projection.
    pub async fn publish_work(&self, work: Option<serde_json::Value>) -> Result<(), ErrorCode> {
        if work.as_ref().is_some_and(|value| !value.is_object()) {
            return Err(ErrorCode::BadRequest);
        }
        let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"work":work}))
            .map_err(|_| ErrorCode::BadRequest)?;
        if bytes.len() > MAX_DOC_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if state.work != bytes {
            state.work = bytes;
            state.bump(&Node::Work);
            state.events.append(b"work\n").await;
        }
        Ok(())
    }
}
