use alan_agent_protocol::{
    AdaptivePresentationHint, ConfirmationYieldPayload, Event, StructuredInputKind,
    StructuredInputOption, StructuredInputQuestion, StructuredInputYieldPayload,
};
use anyhow::Result;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE;
use crate::approval::{PendingConfirmation, append_skill_permission_hints};
use crate::llm::ToolDefinition;

use super::child_run_termination_tool::{
    handle_terminate_child_run, terminate_child_run_tool_definition,
};
use super::delegated_skill_tool::{
    handle_invoke_delegated_skill, invoke_delegated_skill_tool_definition,
};
pub(super) use super::mount_request_tool::parse_mount_request;
use super::mount_request_tool::{handle_request_mount, request_mount_tool_definition};
use super::transition::RuntimeLoopState;
use super::turn_support::{check_turn_cancelled, tool_result_preview};
pub(super) use super::virtual_tool::VirtualToolOutcome;
use crate::agent_machine::NormalizedToolCall;

pub(super) fn virtual_tool_definitions(include_delegated_skill: bool) -> Vec<ToolDefinition> {
    let mut defs = vec![
        request_confirmation_tool_definition(),
        request_mount_tool_definition(),
        request_user_input_tool_definition(),
        update_plan_tool_definition(),
    ];
    if include_delegated_skill {
        defs.push(invoke_delegated_skill_tool_definition());
        defs.push(terminate_child_run_tool_definition());
    }
    defs
}

pub(super) async fn try_handle_virtual_tool_call<E, F>(
    state: &mut RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    tool_arguments: &serde_json::Value,
    cancel: &CancellationToken,
    allow_approved_tool_escalation_execution: bool,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let agent_files = state.agent_files();
    if cancel.is_cancelled()
        && check_turn_cancelled(&mut state.machine, &agent_files, emit, cancel).await?
    {
        return Ok(VirtualToolOutcome::EndTurn);
    }

    match tool_call.name.as_str() {
        "request_confirmation" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(mut pending) = parse_confirmation_request(&tool_call.id, tool_arguments) {
                pending.details =
                    append_skill_permission_hints(pending.details, state.machine.active_skills());
                let request_id = state
                    .agent_files()
                    .write_confirmation_request(&pending)
                    .await?;
                let pending_payload = json!({
                    "status": "pending_confirmation",
                    "request_id": request_id.clone()
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .machine
                    .set_confirmation_for_request(request_id.clone(), pending.clone());
                super::ui_surfaces::paused(&state.agent_files()).await?;
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    payload: serde_json::to_value(ConfirmationYieldPayload {
                        checkpoint_type: pending.checkpoint_type.clone(),
                        summary: pending.summary.clone(),
                        details: Some(pending.details.clone()),
                        default_option: pending
                            .options
                            .iter()
                            .find(|option| option.as_str() == "approve")
                            .cloned()
                            .or_else(|| pending.options.first().cloned()),
                        options: pending.options.clone(),
                        presentation_hints: vec![],
                    })
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid confirmation request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid confirmation request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "request_mount" => handle_request_mount(state, tool_call, tool_arguments, emit).await,
        "request_user_input" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;

            if let Some(request) =
                parse_structured_user_input_request(&tool_call.id, tool_arguments)
            {
                let request_id = state
                    .agent_files()
                    .write_structured_input_request(&request)
                    .await?;
                let pending_payload =
                    json!({"status": "pending_structured_input", "request_id": request_id.clone()});
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(true),
                    result_preview: tool_result_preview(&pending_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    pending_payload,
                    true,
                );
                state
                    .machine
                    .set_structured_input_for_request(request_id.clone(), request.clone());
                super::ui_surfaces::paused(&state.agent_files()).await?;
                emit(Event::Yield {
                    request_id,
                    kind: alan_agent_protocol::YieldKind::StructuredInput,
                    payload: serde_json::to_value(structured_input_yield_payload(
                        request.title.clone(),
                        request.prompt.clone(),
                        request.questions.clone(),
                    ))
                    .unwrap_or_else(|_| json!({})),
                })
                .await;
            } else {
                let error_payload = json!({
                    "status": "invalid_request",
                    "error": "Invalid structured user input request."
                });
                emit(Event::ToolCallCompleted {
                    presentation: None,
                    id: tool_call.id.clone(),
                    name: Some(tool_call.name.clone()),
                    success: Some(false),
                    result_preview: tool_result_preview(&error_payload),
                    audit: None,
                })
                .await;
                state.machine.record_tool_call(
                    &tool_call.name,
                    tool_arguments.clone(),
                    error_payload,
                    false,
                );
                emit(Event::Error {
                    message: "Invalid structured user input request.".to_string(),
                    recoverable: true,
                })
                .await;
                return Ok(VirtualToolOutcome::EndTurn);
            }
            Ok(VirtualToolOutcome::PauseTurn)
        }
        "update_plan" => {
            emit(Event::ToolCallStarted {
                title: None,
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                audit: None,
            })
            .await;
            match parse_plan_update(tool_arguments) {
                Some((explanation, items)) => {
                    state.machine.set_plan_snapshot_at_message_count(
                        explanation.clone(),
                        items.clone(),
                        state.machine.messages().len(),
                    );
                    super::ui_surfaces::plan_updated(
                        &state.agent_files(),
                        explanation.clone(),
                        items.clone(),
                    )
                    .await?;
                    let payload = json!({
                        "status": "plan_updated",
                        "explanation": explanation,
                        "items": items.clone(),
                        "items_count": items.len()
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(true),
                        result_preview: tool_result_preview(&payload),
                        audit: None,
                    })
                    .await;
                    emit(Event::PlanUpdated {
                        explanation: explanation.clone(),
                        items: items.clone(),
                    })
                    .await;
                    state.machine.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        payload.clone(),
                        true,
                    );
                    state
                        .machine
                        .add_tool_message(&tool_call.id, &tool_call.name, payload);
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: true,
                    })
                }
                None => {
                    let error_payload = json!({
                        "status": "invalid_request",
                        "error": "Invalid plan update payload."
                    });
                    emit(Event::ToolCallCompleted {
                        presentation: None,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        success: Some(false),
                        result_preview: tool_result_preview(&error_payload),
                        audit: None,
                    })
                    .await;
                    state.machine.record_tool_call(
                        &tool_call.name,
                        tool_arguments.clone(),
                        error_payload,
                        false,
                    );
                    emit(Event::Error {
                        message: "Invalid plan update payload.".to_string(),
                        recoverable: true,
                    })
                    .await;
                    Ok(VirtualToolOutcome::Continue {
                        refresh_context: false,
                    })
                }
            }
        }
        "invoke_delegated_skill" => {
            let runtime = super::transition::delegated_skill_runtime(state);
            handle_invoke_delegated_skill(runtime, tool_call, tool_arguments, cancel, emit).await
        }
        "terminate_child_run" => {
            handle_terminate_child_run(
                state,
                tool_call,
                tool_arguments,
                allow_approved_tool_escalation_execution,
                emit,
            )
            .await
        }
        _ => Ok(VirtualToolOutcome::NotVirtual),
    }
}

pub(super) fn parse_confirmation_request(
    tool_call_id: &str,
    args: &serde_json::Value,
) -> Option<PendingConfirmation> {
    let checkpoint_type = args
        .get("checkpoint_type")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("confirmation")
        .to_string();
    if checkpoint_type == MOUNT_ESCALATION_CHECKPOINT_TYPE {
        return None;
    }
    let summary = args.get("summary")?.as_str()?.trim().to_string();
    if summary.is_empty() {
        return None;
    }
    let details = args.get("details").cloned().unwrap_or(json!({}));
    let options = args
        .get("options")
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                })
                .collect()
        })
        .filter(|opts: &Vec<String>| !opts.is_empty())
        .unwrap_or_else(|| {
            vec![
                "approve".to_string(),
                "modify".to_string(),
                "reject".to_string(),
            ]
        });

    Some(PendingConfirmation {
        checkpoint_id: tool_call_id.to_string(),
        checkpoint_type,
        summary,
        details,
        options,
    })
}

fn parse_structured_user_input_request(
    tool_call_id: &str,
    arguments: &serde_json::Value,
) -> Option<crate::approval::PendingStructuredInputRequest> {
    let title = arguments.get("title")?.as_str()?.trim().to_string();
    let prompt = arguments.get("prompt")?.as_str()?.trim().to_string();
    if title.is_empty() || prompt.is_empty() {
        return None;
    }
    let request_id = tool_call_id.to_string();

    let questions = arguments
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|raw| {
            let id = parse_non_empty_string(raw.get("id"))?;
            let label = parse_non_empty_string(raw.get("label"))?;
            let prompt = parse_non_empty_string(raw.get("prompt"))?;
            let required = raw
                .get("required")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let options = raw
                .get("options")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|opt| {
                            Some(StructuredInputOption {
                                value: parse_non_empty_string(opt.get("value"))?,
                                label: parse_non_empty_string(opt.get("label"))?,
                                description: parse_optional_string(opt.get("description")),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let kind = parse_structured_input_kind(raw.get("kind"), !options.is_empty())?;
            let placeholder = parse_optional_string(raw.get("placeholder"));
            let help_text = parse_optional_string(raw.get("help_text"));
            let default_value = parse_optional_string(raw.get("default"));
            let default_values = parse_string_array(raw.get("defaults"));
            let min_selected = parse_optional_u32(raw.get("min_selected"));
            let max_selected = parse_optional_u32(raw.get("max_selected"));
            let presentation_hints = parse_presentation_hints(raw.get("presentation_hints"));
            let options = normalize_question_options(kind, options);

            if matches!(
                kind,
                StructuredInputKind::Boolean
                    | StructuredInputKind::SingleSelect
                    | StructuredInputKind::MultiSelect
            ) && options.is_empty()
            {
                return None;
            }

            let option_values = options
                .iter()
                .map(|opt| opt.value.as_str())
                .collect::<Vec<_>>();
            let normalized_default_value = match kind {
                StructuredInputKind::Text
                | StructuredInputKind::Number
                | StructuredInputKind::Integer => default_value.clone(),
                StructuredInputKind::Boolean | StructuredInputKind::SingleSelect => {
                    normalize_single_default(default_value.clone(), option_values.as_slice())
                }
                StructuredInputKind::MultiSelect => None,
            };
            let normalized_default_values = if matches!(kind, StructuredInputKind::MultiSelect) {
                normalize_multi_defaults(
                    default_value.as_deref(),
                    default_values,
                    option_values.as_slice(),
                )
            } else {
                Vec::new()
            };
            let (min_selected, max_selected) =
                normalize_selection_constraints(min_selected, max_selected, options.len());

            Some(StructuredInputQuestion {
                id,
                label,
                prompt,
                kind,
                required,
                placeholder,
                help_text,
                default_value: normalized_default_value,
                default_values: normalized_default_values,
                min_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    min_selected
                } else {
                    None
                },
                max_selected: if matches!(kind, StructuredInputKind::MultiSelect) {
                    max_selected
                } else {
                    None
                },
                options,
                presentation_hints,
            })
        })
        .collect::<Vec<_>>();

    if questions.is_empty() {
        return None;
    }

    Some(crate::approval::PendingStructuredInputRequest {
        request_id,
        title,
        prompt,
        questions,
    })
}

fn parse_non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|raw| raw.as_str())
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(ToString::to_string)
}

fn parse_optional_string(value: Option<&serde_json::Value>) -> Option<String> {
    parse_non_empty_string(value)
}

fn parse_string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| parse_non_empty_string(Some(item)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_optional_u32(value: Option<&serde_json::Value>) -> Option<u32> {
    value
        .and_then(|raw| raw.as_u64())
        .and_then(|raw| u32::try_from(raw).ok())
}

fn parse_structured_input_kind(
    value: Option<&serde_json::Value>,
    has_options: bool,
) -> Option<StructuredInputKind> {
    match value.and_then(|raw| raw.as_str()) {
        Some("text") => Some(StructuredInputKind::Text),
        Some("boolean") => Some(StructuredInputKind::Boolean),
        Some("number") => Some(StructuredInputKind::Number),
        Some("integer") => Some(StructuredInputKind::Integer),
        Some("single_select") => Some(StructuredInputKind::SingleSelect),
        Some("multi_select") => Some(StructuredInputKind::MultiSelect),
        Some(_) => None,
        None => Some(if has_options {
            StructuredInputKind::SingleSelect
        } else {
            StructuredInputKind::Text
        }),
    }
}

fn parse_presentation_hints(value: Option<&serde_json::Value>) -> Vec<AdaptivePresentationHint> {
    value
        .and_then(|raw| raw.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item.as_str() {
                    Some("radio") => Some(AdaptivePresentationHint::Radio),
                    Some("toggle") => Some(AdaptivePresentationHint::Toggle),
                    Some("searchable") => Some(AdaptivePresentationHint::Searchable),
                    Some("multiline") => Some(AdaptivePresentationHint::Multiline),
                    Some("compact") => Some(AdaptivePresentationHint::Compact),
                    Some("dangerous") => Some(AdaptivePresentationHint::Dangerous),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_question_options(
    kind: StructuredInputKind,
    options: Vec<StructuredInputOption>,
) -> Vec<StructuredInputOption> {
    if matches!(kind, StructuredInputKind::Boolean) && options.is_empty() {
        return boolean_options();
    }
    options
}

fn boolean_options() -> Vec<StructuredInputOption> {
    vec![
        StructuredInputOption {
            value: "true".to_string(),
            label: "Yes".to_string(),
            description: None,
        },
        StructuredInputOption {
            value: "false".to_string(),
            label: "No".to_string(),
            description: None,
        },
    ]
}

fn structured_input_yield_payload(
    title: String,
    prompt: String,
    questions: Vec<StructuredInputQuestion>,
) -> StructuredInputYieldPayload {
    StructuredInputYieldPayload {
        title,
        prompt: Some(prompt),
        questions,
    }
}

fn normalize_single_default(
    default_value: Option<String>,
    option_values: &[&str],
) -> Option<String> {
    default_value
        .filter(|value| option_values.is_empty() || option_values.contains(&value.as_str()))
}

fn normalize_multi_defaults(
    default_value: Option<&str>,
    default_values: Vec<String>,
    option_values: &[&str],
) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in default_values {
        if option_values.contains(&value.as_str()) && !normalized.contains(&value) {
            normalized.push(value);
        }
    }

    if normalized.is_empty()
        && let Some(value) = default_value
        && option_values.contains(&value)
    {
        normalized.push(value.to_string());
    }

    normalized
}

fn normalize_selection_constraints(
    min_selected: Option<u32>,
    max_selected: Option<u32>,
    option_count: usize,
) -> (Option<u32>, Option<u32>) {
    let option_limit = u32::try_from(option_count).ok();
    let min = min_selected.filter(|value| Some(*value) <= option_limit);
    let max = max_selected.filter(|value| Some(*value) <= option_limit);

    match (min, max) {
        (Some(min), Some(max)) if max < min => (Some(min), None),
        other => other,
    }
}

fn parse_plan_status(raw: &str) -> Option<alan_agent_protocol::PlanItemStatus> {
    match raw {
        "pending" | "blocked" => Some(alan_agent_protocol::PlanItemStatus::Pending),
        "in_progress" => Some(alan_agent_protocol::PlanItemStatus::InProgress),
        "completed" | "skipped" => Some(alan_agent_protocol::PlanItemStatus::Completed),
        _ => None,
    }
}

fn parse_plan_items(value: &serde_json::Value) -> Option<Vec<alan_agent_protocol::PlanItem>> {
    let items = value.as_array()?;
    let parsed = items
        .iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?.to_string();
            let content = raw
                .get("content")
                .or_else(|| raw.get("description"))?
                .as_str()?
                .to_string();
            let status_raw = raw.get("status")?.as_str()?;
            let status = parse_plan_status(status_raw)?;
            Some(alan_agent_protocol::PlanItem {
                id,
                content,
                status,
            })
        })
        .collect::<Vec<_>>();
    (!parsed.is_empty()).then_some(parsed)
}

fn parse_plan_update(
    arguments: &serde_json::Value,
) -> Option<(Option<String>, Vec<alan_agent_protocol::PlanItem>)> {
    let explanation = arguments
        .get("explanation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let items = parse_plan_items(arguments.get("items")?)?;
    Some((explanation, items))
}

fn request_confirmation_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_confirmation".to_string(),
        description: "Request user confirmation or approval before proceeding with a significant action. Use this when you need explicit user approval before making changes or proceeding with a recommendation.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "checkpoint_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "checkpoint_type": {
                    "type": "string",
                    "description": "The type of checkpoint (e.g., 'business_understanding', 'supplier_recommendation', 'final_confirmation'). Defaults to 'confirmation'."
                },
                "summary": {
                    "type": "string",
                    "description": "A clear summary of what is being proposed or what the user should confirm"
                },
                "details": {
                    "type": "object",
                    "description": "Additional structured details relevant to the confirmation"
                }
            },
            "required": ["summary"]
        }),
    }
}

fn request_user_input_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "request_user_input".to_string(),
        description: "Request structured user input (questions/options) from the client UI and wait for a structured response before continuing.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "request_id": {
                    "type": "string",
                    "description": "Optional legacy field. Runtime uses the tool call id as request_id."
                },
                "title": { "type": "string" },
                "prompt": { "type": "string" },
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "label": { "type": "string" },
                            "prompt": { "type": "string" },
                            "kind": {
                                "type": "string",
                                "enum": ["text", "boolean", "number", "integer", "single_select", "multi_select"]
                            },
                            "required": { "type": "boolean" },
                            "placeholder": { "type": "string" },
                            "help_text": { "type": "string" },
                            "presentation_hints": {
                                "type": "array",
                                "items": {
                                    "type": "string",
                                    "enum": ["radio", "toggle", "searchable", "multiline", "compact", "dangerous"]
                                }
                            },
                            "default": { "type": "string" },
                            "defaults": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "min_selected": { "type": "integer", "minimum": 0 },
                            "max_selected": { "type": "integer", "minimum": 0 },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": { "type": "string" },
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["value", "label"]
                                }
                            }
                        },
                        "required": ["id", "label", "prompt"]
                    }
                }
            },
            "required": ["title", "prompt", "questions"]
        }),
    }
}

fn update_plan_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "update_plan".to_string(),
        description: "Publish a normalized plan/progress update to the client UI. Use this when the task plan changes or step status changes.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "explanation": { "type": "string" },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;

    #[test]
    fn definitions_exclude_optional_delegation_tools_by_default() {
        let defs = virtual_tool_definitions(false);
        assert_eq!(defs.len(), 4);
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_confirmation")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_mount")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "request_user_input")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "update_plan")
        );
        assert!(
            !defs
                .iter()
                .any(|definition| definition.name == "invoke_delegated_skill")
        );
    }

    #[test]
    fn definitions_include_delegation_tools_when_supported() {
        let defs = virtual_tool_definitions(true);
        assert!(
            defs.iter()
                .any(|definition| definition.name == "invoke_delegated_skill")
        );
        assert!(
            defs.iter()
                .any(|definition| definition.name == "terminate_child_run")
        );
    }
}

#[cfg(test)]
#[path = "virtual_tools_tests.rs"]
mod tests;
