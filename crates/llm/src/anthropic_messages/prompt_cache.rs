//! Prompt caching breakpoint placement for Anthropic Messages requests.
//!
//! Anthropic allows at most four `cache_control` breakpoints per request. We place
//! ephemeral breakpoints on the stable prefix in priority order:
//! 1. The system prompt block (when present).
//! 2. The last tool definition (when present).
//! 3. Up to the two most recent user / tool-result turns (last cacheable block per turn).

use super::wire::{AnthropicMessagesRequest, CacheControl, SystemBlock, SystemPrompt};

pub(super) const MAX_CACHE_BREAKPOINTS: usize = 4;
const MAX_RECENT_USER_TURN_BREAKPOINTS: usize = 2;

/// Annotates `request` with up to [`MAX_CACHE_BREAKPOINTS`] ephemeral cache breakpoints.
pub(super) fn apply_prompt_cache(request: &mut AnthropicMessagesRequest) {
    let mut used = 0usize;

    if let Some(system) = request.system.take() {
        let updated = match system {
            SystemPrompt::Text(text) if !text.trim().is_empty() => {
                used += 1;
                let mut block = SystemBlock::text(text);
                block.cache_control = Some(CacheControl::ephemeral());
                SystemPrompt::Blocks(vec![block])
            }
            SystemPrompt::Blocks(mut blocks) if !blocks.is_empty() => {
                if let Some(last) = blocks.last_mut() {
                    last.cache_control = Some(CacheControl::ephemeral());
                    used += 1;
                }
                SystemPrompt::Blocks(blocks)
            }
            other => other,
        };
        request.system = Some(updated);
    }

    if used < MAX_CACHE_BREAKPOINTS
        && let Some(tools) = request.tools.as_mut()
        && let Some(last_tool) = tools.last_mut()
    {
        last_tool.cache_control = Some(CacheControl::ephemeral());
        used += 1;
    }

    let mut remaining_user_turns = MAX_CACHE_BREAKPOINTS
        .saturating_sub(used)
        .min(MAX_RECENT_USER_TURN_BREAKPOINTS);
    if remaining_user_turns == 0 {
        return;
    }

    for message in request.messages.iter_mut().rev() {
        if remaining_user_turns == 0 {
            break;
        }
        if message.role != "user" {
            continue;
        }
        if let Some(slot) = message
            .content
            .iter_mut()
            .rev()
            .find_map(|block| block.cache_control_mut())
        {
            *slot = Some(CacheControl::ephemeral());
            remaining_user_turns -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic_messages::wire::{
        AnthropicMessagesMessage, AnthropicMessagesToolDefinition, ContentBlockInput,
    };
    use serde_json::{Value, json};

    fn count_cache_controls(value: &Value) -> usize {
        match value {
            Value::Object(map) => {
                usize::from(map.contains_key("cache_control"))
                    + map.values().map(count_cache_controls).sum::<usize>()
            }
            Value::Array(items) => items.iter().map(count_cache_controls).sum(),
            _ => 0,
        }
    }

    #[test]
    fn places_four_breakpoints_on_system_last_tool_and_two_recent_user_turns() {
        let mut request = AnthropicMessagesRequest {
            model: "claude-sonnet-4-5".into(),
            messages: vec![
                AnthropicMessagesMessage::user_text("turn 1"),
                AnthropicMessagesMessage::assistant_text("reply 1"),
                AnthropicMessagesMessage::user_text("turn 2"),
                AnthropicMessagesMessage::assistant_text("reply 2"),
                AnthropicMessagesMessage::user_tool_result("toolu_1", "{\"ok\":true}".into()),
            ],
            max_tokens: 1024,
            system: Some(SystemPrompt::Text("be concise".into())),
            temperature: None,
            tools: Some(vec![
                AnthropicMessagesToolDefinition::new("first", "one", json!({"type": "object"})),
                AnthropicMessagesToolDefinition::new("second", "two", json!({"type": "object"})),
            ]),
            stream: None,
            thinking: None,
            output_config: None,
        };

        apply_prompt_cache(&mut request);
        let value = serde_json::to_value(&request).unwrap();
        let ephemeral = json!({"type": "ephemeral"});

        assert_eq!(count_cache_controls(&value), 4);
        assert_eq!(
            value["system"],
            json!([{"type": "text", "text": "be concise", "cache_control": {"type": "ephemeral"}}])
        );
        assert!(value["tools"][0].get("cache_control").is_none());
        assert_eq!(value["tools"][1]["cache_control"], ephemeral);
        // Oldest user turn (index 0) stays unmarked; two most recent user/tool_result turns (2 and 4) are marked.
        assert!(
            value["messages"][0]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert!(
            value["messages"][1]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            value["messages"][2]["content"][0]["cache_control"],
            ephemeral
        );
        assert!(
            value["messages"][3]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            value["messages"][4]["content"][0]["cache_control"],
            ephemeral
        );
    }

    #[test]
    fn marks_only_last_block_of_multi_block_user_turn_and_never_exceeds_four() {
        let mut request = AnthropicMessagesRequest {
            model: "claude-sonnet-4-5".into(),
            messages: vec![
                AnthropicMessagesMessage::user_text("old"),
                AnthropicMessagesMessage::assistant_text("ok"),
                AnthropicMessagesMessage {
                    role: "user".into(),
                    content: vec![
                        ContentBlockInput::Text {
                            text: "look".into(),
                            cache_control: None,
                        },
                        ContentBlockInput::Image {
                            source: json!({"type": "url", "url": "https://example.com/a.png"}),
                            cache_control: None,
                        },
                    ],
                },
            ],
            max_tokens: 1024,
            system: None,
            temperature: None,
            tools: None,
            stream: None,
            thinking: None,
            output_config: None,
        };

        apply_prompt_cache(&mut request);
        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(count_cache_controls(&value), 2);
        assert!(
            value["messages"][2]["content"][0]
                .get("cache_control")
                .is_none()
        );
        assert_eq!(
            value["messages"][2]["content"][1]["cache_control"],
            json!({"type": "ephemeral"})
        );
    }
}
