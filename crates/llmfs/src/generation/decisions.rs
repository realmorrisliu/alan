//! First-class typed Decision requests (`decision.v1`) over the Connection operation lifecycle.

use std::collections::BTreeMap;
use std::time::Duration;

use alan_llm::{
    CalibrationKind, DecisionLevel, DecisionOption, DecisionQuestion, DecisionRequest,
    DecisionResponse,
};
use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u16,
    schema: String,
    state: serde_json::Value,
    #[serde(deserialize_with = "deserialize_unique_questions")]
    questions: BTreeMap<String, WireDecisionQuestion>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    model_override: Option<String>,
    deadline_ms: u64,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireDecisionQuestion {
    Predicate {
        prompt: String,
    },
    Choice {
        prompt: String,
        options: Vec<WireDecisionOption>,
    },
    Score {
        prompt: String,
        levels: Vec<WireDecisionLevel>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDecisionOption {
    id: String,
    description: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireDecisionLevel {
    id: String,
    description: String,
    score: f64,
}

fn deserialize_unique_questions<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, WireDecisionQuestion>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct MapVisitor;

    impl<'de> serde::de::Visitor<'de> for MapVisitor {
        type Value = BTreeMap<String, WireDecisionQuestion>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map of unique decision question IDs")
        }

        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, WireDecisionQuestion>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate decision question key"));
                }
            }
            Ok(result)
        }
    }

    deserializer.deserialize_map(MapVisitor)
}

impl From<WireDecisionQuestion> for DecisionQuestion {
    fn from(value: WireDecisionQuestion) -> Self {
        match value {
            WireDecisionQuestion::Predicate { prompt } => Self::Predicate { prompt },
            WireDecisionQuestion::Choice { prompt, options } => Self::Choice {
                prompt,
                options: options
                    .into_iter()
                    .map(|opt| DecisionOption {
                        id: opt.id,
                        description: opt.description,
                    })
                    .collect(),
            },
            WireDecisionQuestion::Score { prompt, levels } => Self::Score {
                prompt,
                levels: levels
                    .into_iter()
                    .map(|lvl| DecisionLevel {
                        id: lvl.id,
                        description: lvl.description,
                        score: lvl.score,
                    })
                    .collect(),
            },
        }
    }
}

impl Request {
    fn validate(self) -> Result<(DecisionRequest, Duration), ()> {
        if self.version != 1
            || self.schema != "decision.v1"
            || !(1..=30_000).contains(&self.deadline_ms)
        {
            return Err(());
        }
        let request = DecisionRequest {
            state: self.state,
            questions: self
                .questions
                .into_iter()
                .map(|(id, question)| (id, question.into()))
                .collect(),
            instructions: self.instructions,
            model_override: self.model_override,
        };
        request.validate().map_err(|_| ())?;
        Ok((request, Duration::from_millis(self.deadline_ms)))
    }
}

pub(super) async fn commit(buf: Vec<u8>, operation: Arc<Generation>) -> Result<(), ErrorCode> {
    let parsed = if buf.is_empty() || buf.len() > crate::MAX_DOC_BYTES {
        Err(())
    } else {
        serde_json::from_slice::<Request>(&buf)
            .map_err(|_| ())
            .and_then(Request::validate)
    };
    let (request, deadline) = match parsed {
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
                provider.decide(request.clone()).await
            }) => result,
        };
        let response = match result {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                fail_generation(
                    &operation,
                    GenStatus::Error,
                    if error.is::<alan_llm::MalformedDecisionResponse>() {
                        "malformed_decision_response"
                    } else {
                        alan_llm::safe_failure_reason(&error)
                    },
                )
                .await;
                return;
            }
            Err(_) => {
                fail_generation(&operation, GenStatus::Error, "decision_timeout").await;
                return;
            }
        };
        publish_result(&operation, &request, response).await;
    });
    Ok(())
}

async fn publish_result(
    operation: &Generation,
    request: &DecisionRequest,
    response: DecisionResponse,
) {
    let calibration_forged = operation.connection.decision_calibration
        == Some(CalibrationKind::Uncalibrated)
        && response.calibration == CalibrationKind::Calibrated;
    if calibration_forged || request.validate_response(&response).is_err() {
        fail_generation(operation, GenStatus::Error, "invalid_decision_answer").await;
        return;
    }
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
        "decision": {
            "schema": "decision.v1",
            "calibration": response.calibration,
            "provider": response.provider,
            "model": response.model,
            "answers": response.answers,
            "usage": response.usage.map(WireTokenUsage::from),
            "cost_microusd": null,
        },
    }));
    operation.events.append(record.as_bytes()).await;
    operation.advance(GenStatus::Done);
}

#[cfg(test)]
mod tests;
