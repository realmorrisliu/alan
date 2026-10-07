//! Finite-choice requests reuse the Connection operation lifecycle, never Tools.

use alan_llm::{
    ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationCandidate, EvaluationSelection,
};
use serde::Deserialize;
use std::time::Duration;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u16,
    schema: String,
    input: String,
    candidates: Vec<Candidate>,
    deadline_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Candidate {
    id: String,
    description: String,
}

impl Request {
    fn validate(self) -> Result<(ChoiceEvaluationRequest, Duration), ()> {
        if self.version != 1
            || self.schema != "choice.v1"
            || !(1..=30_000).contains(&self.deadline_ms)
        {
            return Err(());
        }
        let request = ChoiceEvaluationRequest {
            input: self.input,
            candidates: self
                .candidates
                .into_iter()
                .map(|candidate| EvaluationCandidate {
                    id: candidate.id,
                    description: candidate.description,
                })
                .collect(),
        };
        request.validate().map_err(|_| ())?;
        Ok((request, Duration::from_millis(self.deadline_ms)))
    }
}

pub(super) async fn commit(buf: Vec<u8>, operation: Arc<Generation>) -> Result<(), ErrorCode> {
    let request = serde_json::from_slice::<Request>(&buf)
        .map_err(|_| ())
        .and_then(Request::validate);
    let (request, deadline) = match request {
        Ok(request) => request,
        Err(()) => {
            let _guard = operation.finalize.lock().await;
            if operation.claim(GenStatus::Rejected) {
                operation
                    .events
                    .append(&event_line(WireStreamEventV1::rejected()))
                    .await;
            }
            return Err(ErrorCode::BadRequest);
        }
    };
    if !operation.claim(GenStatus::Running) {
        return Err(ErrorCode::BadRequest);
    }
    let expires = tokio::time::Instant::now() + deadline;
    tokio::spawn(async move {
        let result = tokio::select! {
            biased;
            _ = operation.abort.notified() => return,
            result = tokio::time::timeout_at(expires, async {
                let mut provider = operation.connection.provider.lock().await;
                provider.evaluate_choice(request.clone()).await
            }) => result,
        };
        let response = match result {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                fail_generation(
                    &operation,
                    GenStatus::Error,
                    alan_llm::safe_failure_reason(&error),
                )
                .await;
                return;
            }
            Err(_) => {
                fail_generation(&operation, GenStatus::Error, "evaluation_timeout").await;
                return;
            }
        };
        publish_result(&operation, &request, response).await;
    });
    Ok(())
}

async fn publish_result(
    operation: &Generation,
    request: &ChoiceEvaluationRequest,
    response: ChoiceEvaluationResponse,
) {
    if request.validate_selection(&response.selection).is_err() {
        fail_generation(operation, GenStatus::Error, "invalid_evaluation_selection").await;
        return;
    }
    let selection = match response.selection {
        EvaluationSelection::Selected(id) => serde_json::json!({"kind": "selected", "id": id}),
        EvaluationSelection::NoMatch => serde_json::json!({"kind": "no_match"}),
    };
    let _guard = operation.finalize.lock().await;
    if operation.is_terminal() {
        return;
    }
    if let Some(usage) = response.usage {
        operation.record_usage(usage);
    }
    let record = render_json_doc(serde_json::json!({
        "version": 1,
        "done": true,
        "evaluation": {
            "schema": "choice.v1",
            "selection": selection,
            "provider": operation.connection.provider_name,
            "model": operation.connection.model,
            "usage": response.usage.map(WireTokenUsage::from),
            "cost_microusd": null,
        },
    }));
    operation.events.append(record.as_bytes()).await;
    operation.advance(GenStatus::Done);
}

#[cfg(test)]
mod tests;
