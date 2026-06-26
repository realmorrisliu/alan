//! Tape — the AI Turing Machine's conversation tape.
//!
//! The tape holds the ordered sequence of messages (user, assistant, tool),
//! optional compaction summary, and reference context items.
//!
//! # Two-Layer Content Model
//!
//! The tape uses a two-layer abstraction:
//! - **ContentPart** — "nouns": the symbols on the tape (text, thinking, attachments, structured data)
//! - **ToolRequest / ToolResponse** — "verbs": the read/write head's actions
//!
//! This separation prevents category confusion between passive content and active instructions.

use crate::approval::{
    RUNTIME_CONFIRMATION_CONTROL_SOURCE, RUNTIME_CONFIRMATION_CONTROL_VERSION,
    runtime_confirmation_control_kind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

// ============================================================================
// Layer 1: Content symbols — re-exported from alan-agent-protocol
// ============================================================================

pub use alan_agent_protocol::{ContentPart, parts_to_text};

// ============================================================================
// Layer 2: Actions — the read/write head's instructions
// ============================================================================

/// A tool call request issued by the assistant — the read/write head's action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A tool execution response. The result itself is a composition of content parts,
/// so tools can return rich content (screenshots, structured data) natively.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResponse {
    pub id: String,
    pub content: Vec<ContentPart>,
}

impl ToolResponse {
    /// Create a tool response with a single text result.
    pub fn text(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: vec![ContentPart::text(text)],
        }
    }

    /// Create a tool response with structured data.
    pub fn structured(id: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            content: vec![ContentPart::structured(data)],
        }
    }

    /// Get the concatenated text content of this response.
    /// Structured parts are serialized to JSON strings.
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .map(|p| p.to_text_lossy())
            .collect::<Vec<_>>()
            .join("")
    }
}

// ============================================================================
// Message — the complete tape record
// ============================================================================

/// A message on the tape. The role is encoded in the enum variant,
/// eliminating the old flat struct with optional fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// User input (can contain text, attachments, structured data).
    User { parts: Vec<ContentPart> },

    /// Assistant output (content + optional tool call requests).
    Assistant {
        parts: Vec<ContentPart>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_requests: Vec<ToolRequest>,
    },

    /// Tool execution results.
    Tool { responses: Vec<ToolResponse> },

    /// System instructions (system prompt, context injection, etc.)
    System { parts: Vec<ContentPart> },

    /// Reference context (injected context items, compaction summaries).
    /// Separated from System because it has different lifecycle semantics.
    Context { parts: Vec<ContentPart> },
}

/// Role of the message sender — derived from the Message variant.
/// Kept for backward compatibility and convenience in pattern matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    Context,
    User,
    Assistant,
    Tool,
}

impl Message {
    // -- Constructors --------------------------------------------------------

    /// Create a user message with text content.
    pub fn user(text: impl Into<String>) -> Self {
        Message::User {
            parts: vec![ContentPart::text(text)],
        }
    }

    /// Create a user message with multiple content parts.
    pub fn user_parts(parts: Vec<ContentPart>) -> Self {
        Message::User { parts }
    }

    /// Create an assistant message with text content.
    pub fn assistant(text: impl Into<String>) -> Self {
        Message::Assistant {
            parts: vec![ContentPart::text(text)],
            tool_requests: vec![],
        }
    }

    /// Create an assistant message with text and tool requests.
    pub fn assistant_with_tools(text: impl Into<String>, tool_requests: Vec<ToolRequest>) -> Self {
        Message::Assistant {
            parts: vec![ContentPart::text(text)],
            tool_requests,
        }
    }

    /// Create a tool result message with a single text response.
    pub fn tool_text(id: impl Into<String>, text: impl Into<String>) -> Self {
        Message::Tool {
            responses: vec![ToolResponse::text(id, text)],
        }
    }

    /// Create a tool result message with structured data.
    pub fn tool_structured(id: impl Into<String>, data: serde_json::Value) -> Self {
        Message::Tool {
            responses: vec![ToolResponse::structured(id, data)],
        }
    }

    /// Create a tool result message with multiple responses.
    pub fn tool_multi(responses: Vec<ToolResponse>) -> Self {
        Message::Tool { responses }
    }

    /// Create a system message.
    pub fn system(text: impl Into<String>) -> Self {
        Message::System {
            parts: vec![ContentPart::text(text)],
        }
    }

    /// Create a context message (for reference context, summaries, etc.)
    pub fn context(text: impl Into<String>) -> Self {
        Message::Context {
            parts: vec![ContentPart::text(text)],
        }
    }

    // -- Accessors -----------------------------------------------------------

    /// Get the role of this message.
    pub fn role(&self) -> MessageRole {
        match self {
            Message::User { .. } => MessageRole::User,
            Message::Assistant { .. } => MessageRole::Assistant,
            Message::Tool { .. } => MessageRole::Tool,
            Message::System { .. } => MessageRole::System,
            Message::Context { .. } => MessageRole::Context,
        }
    }

    /// Get the content parts of this message.
    /// For Tool messages, returns an empty slice (use `tool_responses()` instead).
    pub fn parts(&self) -> &[ContentPart] {
        match self {
            Message::User { parts }
            | Message::Assistant { parts, .. }
            | Message::System { parts }
            | Message::Context { parts } => parts,
            Message::Tool { .. } => &[],
        }
    }

    /// Get the tool requests from an assistant message.
    pub fn tool_requests(&self) -> &[ToolRequest] {
        match self {
            Message::Assistant { tool_requests, .. } => tool_requests,
            _ => &[],
        }
    }

    /// Get the tool responses from a tool message.
    pub fn tool_responses(&self) -> &[ToolResponse] {
        match self {
            Message::Tool { responses } => responses,
            _ => &[],
        }
    }

    /// Get the concatenated text content of this message.
    /// For structured/attachment parts, serializes them to string.
    /// For Tool messages, concatenates all response text.
    pub fn text_content(&self) -> String {
        match self {
            Message::User { parts }
            | Message::Assistant { parts, .. }
            | Message::System { parts }
            | Message::Context { parts } => parts
                .iter()
                .map(|p| p.to_text_lossy())
                .collect::<Vec<_>>()
                .join(""),
            Message::Tool { responses } => responses
                .iter()
                .map(|r| r.text_content())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Get the thinking content from an assistant message, if any.
    pub fn thinking_content(&self) -> Option<String> {
        match self {
            Message::Assistant { parts, .. } => {
                let thinking: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Thinking { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");
                if thinking.is_empty() {
                    None
                } else {
                    Some(thinking)
                }
            }
            _ => None,
        }
    }

    /// Get the latest thinking signature from an assistant message, if any.
    pub fn thinking_signature(&self) -> Option<String> {
        match self {
            Message::Assistant { parts, .. } => parts.iter().rev().find_map(|p| match p {
                ContentPart::Thinking { signature, .. } => signature
                    .as_deref()
                    .filter(|sig| !sig.trim().is_empty())
                    .map(ToString::to_string),
                _ => None,
            }),
            _ => None,
        }
    }

    /// Get redacted thinking blocks from an assistant message.
    pub fn redacted_thinking_blocks(&self) -> Vec<String> {
        match self {
            Message::Assistant { parts, .. } => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::RedactedThinking { data } => Some(data.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Get the non-thinking text content from an assistant message.
    /// For non-assistant messages, behaves like text_content().
    pub fn non_thinking_text_content(&self) -> String {
        match self {
            Message::Assistant { parts, .. } => parts
                .iter()
                .filter(|p| {
                    !matches!(
                        p,
                        ContentPart::Thinking { .. } | ContentPart::RedactedThinking { .. }
                    )
                })
                .map(|p| p.to_text_lossy())
                .collect::<Vec<_>>()
                .join(""),
            _ => self.text_content(),
        }
    }

    /// Check if this message is a user message.
    pub fn is_user(&self) -> bool {
        matches!(self, Message::User { .. })
    }

    /// Check if this message is an assistant message.
    pub fn is_assistant(&self) -> bool {
        matches!(self, Message::Assistant { .. })
    }

    /// Check if this message is a tool message.
    pub fn is_tool(&self) -> bool {
        matches!(self, Message::Tool { .. })
    }

    /// Check if this message is a system message.
    pub fn is_system(&self) -> bool {
        matches!(self, Message::System { .. })
    }

    /// Check if this message is a context message.
    pub fn is_context(&self) -> bool {
        matches!(self, Message::Context { .. })
    }
}

pub const SUMMARY_PREFIX: &str = "The following is a compacted summary of the earlier conversation history in this session. Use this context to continue the work seamlessly without duplicating what has already been done:";

#[derive(Debug, Clone)]
pub struct Tape {
    reference_context: ReferenceContextState,
    messages: Vec<Message>,
    messages_token_estimate: usize,
    summary: Option<String>,
    summary_token_estimate: usize,
    /// Number of times compaction has been applied in this session.
    compaction_count: usize,
}

#[derive(Debug, Clone, Default)]
struct ReferenceContextState {
    items: Vec<ContextItem>,
    rendered_messages: Vec<Message>,
    rendered_token_estimate: usize,
    revision: u64,
    last_delta: ContextItemsDelta,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextItemsDelta {
    pub changed: bool,
    pub reordered: bool,
    pub revision: u64,
    pub added_ids: Vec<String>,
    pub updated_ids: Vec<String>,
    pub removed_ids: Vec<String>,
}

impl ContextItemsDelta {
    pub fn is_empty(&self) -> bool {
        !self.changed
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceContextSnapshot {
    pub revision: u64,
    pub item_count: usize,
    pub delta: ContextItemsDelta,
}

#[derive(Debug, Clone)]
pub struct PromptContextView {
    pub messages: Vec<Message>,
    pub estimated_tokens: usize,
    pub reference_context: ReferenceContextSnapshot,
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub fingerprint: String,
}

impl ContextItem {
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let id = id.into();
        let kind = kind.into();
        let title = title.into();
        let content = content.into();
        let fingerprint = fingerprint_context(&kind, &title, &content);
        Self {
            id,
            kind,
            title,
            content,
            fingerprint,
        }
    }

    pub fn with_fingerprint(
        id: impl Into<String>,
        kind: impl Into<String>,
        title: impl Into<String>,
        content: impl Into<String>,
        fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            content: content.into(),
            fingerprint: fingerprint.into(),
        }
    }

    fn ensure_fingerprint_matches(&mut self) {
        let expected = fingerprint_context(&self.kind, &self.title, &self.content);
        if self.fingerprint != expected {
            self.fingerprint = expected;
        }
    }
}

impl Default for Tape {
    fn default() -> Self {
        Self::new()
    }
}

impl Tape {
    pub fn new() -> Self {
        Self {
            reference_context: ReferenceContextState::default(),
            messages: Vec::new(),
            messages_token_estimate: 0,
            summary: None,
            summary_token_estimate: 0,
            compaction_count: 0,
        }
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the current compaction summary, if any.
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Get the number of compactions applied in this session.
    pub fn compaction_count(&self) -> usize {
        self.compaction_count
    }

    pub fn prompt_view(&self) -> PromptContextView {
        let mut out = Vec::with_capacity(
            self.reference_context.rendered_messages.len()
                + usize::from(self.summary.is_some())
                + self.messages.len(),
        );
        out.extend(self.reference_context.rendered_messages.clone());
        if let Some(summary) = &self.summary {
            out.push(summary_prompt_message(summary));
        }
        out.extend(self.messages.clone());

        PromptContextView {
            messages: out,
            estimated_tokens: self.estimated_prompt_tokens(),
            reference_context: ReferenceContextSnapshot {
                revision: self.reference_context.revision,
                item_count: self.reference_context.items.len(),
                delta: self.reference_context.last_delta.clone(),
            },
        }
    }

    /// Backward-compatible wrapper for callers/tests that only need the prompt messages.
    pub fn messages_for_prompt(&self) -> Vec<Message> {
        self.prompt_view().messages
    }

    /// Lightweight token estimate for prompt budgeting and compaction heuristics.
    /// We intentionally avoid provider-specific tokenizers here to keep runtime cost low.
    pub fn estimated_prompt_tokens(&self) -> usize {
        self.reference_context.rendered_token_estimate
            + self.summary_token_estimate
            + self.messages_token_estimate
    }

    pub fn push(&mut self, message: Message) {
        self.messages_token_estimate += estimate_message_tokens(&message);
        self.messages.push(message);
    }

    pub fn clear(&mut self) {
        self.reference_context = ReferenceContextState::default();
        self.messages.clear();
        self.messages_token_estimate = 0;
        self.summary = None;
        self.summary_token_estimate = 0;
        self.compaction_count = 0;
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn replace(&mut self, messages: Vec<Message>) {
        self.messages = messages;
        self.messages_token_estimate = self
            .messages
            .iter()
            .map(estimate_message_tokens)
            .sum::<usize>();
    }

    pub fn context_items(&self) -> &[ContextItem] {
        &self.reference_context.items
    }

    pub fn last_context_delta(&self) -> &ContextItemsDelta {
        &self.reference_context.last_delta
    }

    pub fn context_revision(&self) -> u64 {
        self.reference_context.revision
    }

    /// Apply a new reference context snapshot and return a diff against the previous snapshot.
    pub fn apply_context_items(&mut self, items: Vec<ContextItem>) -> ContextItemsDelta {
        let items = normalize_context_items(items);
        let delta = diff_context_items(
            &self.reference_context.items,
            &items,
            self.reference_context.revision,
        );

        if delta.changed {
            let (rendered_messages, rendered_token_estimate) = render_context_items(&items);
            self.reference_context.items = items;
            self.reference_context.rendered_messages = rendered_messages;
            self.reference_context.rendered_token_estimate = rendered_token_estimate;
            self.reference_context.revision = delta.revision;
        }

        self.reference_context.last_delta = delta.clone();
        delta
    }

    pub fn set_summary(&mut self, summary: String) {
        self.summary_token_estimate = estimate_message_tokens(&summary_prompt_message(&summary));
        self.summary = Some(summary);
    }

    pub fn clear_summary(&mut self) {
        self.summary = None;
        self.summary_token_estimate = 0;
    }

    pub(crate) fn compaction_retention_start(&self, keep_last: usize) -> usize {
        compaction_retention_start(&self.messages, keep_last)
    }

    pub fn compact(&mut self, summary: String, keep_last: usize) {
        let retention_start = self.compaction_retention_start(keep_last);
        self.messages = self.messages[retention_start..].to_vec();
        self.messages_token_estimate = self
            .messages
            .iter()
            .map(estimate_message_tokens)
            .sum::<usize>();
        self.set_summary(summary);
        self.compaction_count += 1;
    }
}

fn summary_prompt_message(summary: &str) -> Message {
    Message::context(format!("{SUMMARY_PREFIX}\n{}", summary))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MessageSpan {
    start: usize,
    end: usize,
    kind: SpanKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanKind {
    UserTurn,
    Control,
}

fn compaction_retention_start(messages: &[Message], keep_last: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }

    if keep_last == 0 {
        return messages.len();
    }

    let spans = semantic_message_spans(messages);
    let Some(last_span) = spans.last().copied() else {
        return 0;
    };

    let mut retention_start = last_span.start;
    let mut retained_messages = last_span.end - last_span.start;

    for span in spans.iter().rev().skip(1) {
        let span_len = span.end - span.start;
        if retained_messages + span_len > keep_last {
            break;
        }
        retention_start = span.start;
        retained_messages += span_len;
    }

    let last_span_oversized = (last_span.end - last_span.start) > keep_last;

    // Prefer complete recent spans when they fit within the raw-message retention budget, but
    // fall back to the raw tail when the latest semantic span alone exceeds that budget.
    let retention_start = if last_span_oversized && keep_last < messages.len() {
        messages.len().saturating_sub(keep_last)
    } else {
        retention_start
    };

    align_retention_start_to_tool_block(messages, retention_start)
}

fn align_retention_start_to_tool_block(messages: &[Message], retention_start: usize) -> usize {
    if retention_start >= messages.len()
        || !matches!(messages[retention_start], Message::Tool { .. })
    {
        return retention_start;
    }

    let mut first_tool_idx = retention_start;
    while first_tool_idx > 0 && matches!(messages[first_tool_idx - 1], Message::Tool { .. }) {
        first_tool_idx -= 1;
    }

    if first_tool_idx > 0
        && let Message::Assistant { tool_requests, .. } = &messages[first_tool_idx - 1]
        && !tool_requests.is_empty()
    {
        return first_tool_idx - 1;
    }

    retention_start
}

fn semantic_message_spans(messages: &[Message]) -> Vec<MessageSpan> {
    let mut spans = Vec::new();
    let mut start = 0usize;

    while start < messages.len() {
        let kind = if is_non_control_user_turn_boundary(&messages[start]) {
            SpanKind::UserTurn
        } else {
            SpanKind::Control
        };
        let mut end = start + 1;
        while end < messages.len() && !is_non_control_user_turn_boundary(&messages[end]) {
            end += 1;
        }
        spans.push(MessageSpan { start, end, kind });
        start = end;
    }

    spans
}

fn is_non_control_user_turn_boundary(message: &Message) -> bool {
    message.is_user() && !is_internal_control_message(message)
}

fn is_internal_control_message(message: &Message) -> bool {
    match message {
        Message::User { parts } => parts.iter().any(is_internal_control_part),
        _ => false,
    }
}

fn is_internal_control_part(part: &ContentPart) -> bool {
    match part {
        ContentPart::Structured { data } => is_internal_control_payload(data),
        ContentPart::Text { text } => serde_json::from_str::<serde_json::Value>(text.trim())
            .map(|payload| is_internal_control_payload(&payload))
            .unwrap_or(false),
        _ => false,
    }
}

fn is_internal_control_payload(payload: &serde_json::Value) -> bool {
    let checkpoint_type = payload
        .get("checkpoint_type")
        .and_then(serde_json::Value::as_str);
    let Some(expected_kind) = checkpoint_type.and_then(runtime_confirmation_control_kind) else {
        return false;
    };

    let marker = payload.get("__alan_internal_control");
    let marker_kind = marker
        .and_then(|value| value.get("kind"))
        .and_then(serde_json::Value::as_str);
    let marker_version = marker
        .and_then(|value| value.get("version"))
        .and_then(serde_json::Value::as_u64);
    let marker_source = marker
        .and_then(|value| value.get("source"))
        .and_then(serde_json::Value::as_str);

    marker_kind == Some(expected_kind)
        && marker_version == Some(RUNTIME_CONFIRMATION_CONTROL_VERSION)
        && marker_source == Some(RUNTIME_CONFIRMATION_CONTROL_SOURCE)
}

fn normalize_context_items(mut items: Vec<ContextItem>) -> Vec<ContextItem> {
    for item in &mut items {
        item.ensure_fingerprint_matches();
    }
    items
}

fn diff_context_items(
    old_items: &[ContextItem],
    new_items: &[ContextItem],
    current_revision: u64,
) -> ContextItemsDelta {
    let old_map: HashMap<&str, &str> = old_items
        .iter()
        .map(|item| (item.id.as_str(), item.fingerprint.as_str()))
        .collect();
    let new_map: HashMap<&str, &str> = new_items
        .iter()
        .map(|item| (item.id.as_str(), item.fingerprint.as_str()))
        .collect();

    let mut added_ids = Vec::new();
    let mut updated_ids = Vec::new();
    let mut removed_ids = Vec::new();

    for item in new_items {
        match old_map.get(item.id.as_str()) {
            None => added_ids.push(item.id.clone()),
            Some(old_fp) if *old_fp != item.fingerprint => updated_ids.push(item.id.clone()),
            _ => {}
        }
    }

    for item in old_items {
        if !new_map.contains_key(item.id.as_str()) {
            removed_ids.push(item.id.clone());
        }
    }

    let old_order: Vec<&str> = old_items.iter().map(|i| i.id.as_str()).collect();
    let new_order: Vec<&str> = new_items.iter().map(|i| i.id.as_str()).collect();
    let reordered = old_order != new_order
        && added_ids.is_empty()
        && updated_ids.is_empty()
        && removed_ids.is_empty();

    let changed =
        reordered || !added_ids.is_empty() || !updated_ids.is_empty() || !removed_ids.is_empty();

    ContextItemsDelta {
        changed,
        reordered,
        revision: if changed {
            current_revision.saturating_add(1)
        } else {
            current_revision
        },
        added_ids,
        updated_ids,
        removed_ids,
    }
}

fn render_context_items(items: &[ContextItem]) -> (Vec<Message>, usize) {
    let mut rendered_messages = Vec::new();
    let mut token_estimate = 0;

    for item in items {
        let content = format_context_item(item);
        if content.is_empty() {
            continue;
        }
        let message = Message::context(content);
        token_estimate += estimate_message_tokens(&message);
        rendered_messages.push(message);
    }

    (rendered_messages, token_estimate)
}

fn estimate_message_tokens(message: &Message) -> usize {
    // Rough heuristic: ~4 chars/token plus a small per-message framing overhead.
    let parts_tokens: usize = message
        .parts()
        .iter()
        .map(estimate_content_part_tokens)
        .sum();

    let tool_requests_tokens: usize = message
        .tool_requests()
        .iter()
        .map(estimate_tool_request_tokens)
        .sum();

    let tool_responses_tokens: usize = message
        .tool_responses()
        .iter()
        .map(|r| {
            r.content
                .iter()
                .map(estimate_content_part_tokens)
                .sum::<usize>()
                + estimate_text_tokens(&r.id)
                + 4
        })
        .sum();

    parts_tokens + tool_requests_tokens + tool_responses_tokens + 6
}

fn estimate_content_part_tokens(part: &ContentPart) -> usize {
    match part {
        ContentPart::Text { text } | ContentPart::Thinking { text, .. } => {
            estimate_text_tokens(text)
        }
        ContentPart::RedactedThinking { data } => estimate_text_tokens(data),
        ContentPart::Attachment {
            hash, mime_type, ..
        } => estimate_text_tokens(hash) + estimate_text_tokens(mime_type) + 10,
        ContentPart::Structured { data } => estimate_json_tokens(data),
    }
}

fn estimate_tool_request_tokens(req: &ToolRequest) -> usize {
    estimate_text_tokens(&req.id)
        + estimate_text_tokens(&req.name)
        + estimate_json_tokens(&req.arguments)
        + 4
}

fn estimate_json_tokens(value: &serde_json::Value) -> usize {
    estimate_text_tokens(&value.to_string())
}

pub(crate) fn estimate_user_message_tokens(parts: &[ContentPart]) -> usize {
    parts
        .iter()
        .map(estimate_content_part_tokens)
        .sum::<usize>()
        + 6
}

pub(crate) fn estimate_text_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    chars.div_ceil(4)
}

pub fn format_context_item(item: &ContextItem) -> String {
    let content = item.content.trim();
    if content.is_empty() {
        return String::new();
    }
    format!("{}:\n{}", item.title.trim(), content)
}

pub fn fingerprint_context(kind: &str, title: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(b"\n");
    hasher.update(title.as_bytes());
    hasher.update(b"\n");
    hasher.update(content.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: MessageRole, content: &str) -> Message {
        match role {
            MessageRole::User => Message::user(content),
            MessageRole::Assistant => Message::assistant(content),
            MessageRole::System => Message::system(content),
            MessageRole::Context => Message::context(content),
            MessageRole::Tool => {
                // For test convenience, create a tool message with a single text response
                Message::Tool {
                    responses: vec![ToolResponse::text("test", content)],
                }
            }
        }
    }

    fn assistant_with_tool_request(id: &str, name: &str) -> Message {
        Message::assistant_with_tools(
            "",
            vec![ToolRequest {
                id: id.to_string(),
                name: name.to_string(),
                arguments: serde_json::json!({}),
            }],
        )
    }

    fn item(id: &str, content: &str) -> ContextItem {
        ContextItem {
            id: id.to_string(),
            kind: "test".to_string(),
            title: format!("Title {}", id),
            content: content.to_string(),
            fingerprint: fingerprint_context("test", &format!("Title {}", id), content),
        }
    }

    fn control_user_message() -> Message {
        Message::user_parts(vec![ContentPart::structured(serde_json::json!({
            "checkpoint_id": "tool_escalation_call-1",
            "checkpoint_type": "tool_escalation",
            "choice": "approve",
            "__alan_internal_control": {
                "kind": "tool_escalation_confirmation",
                "version": 1,
                "source": "runtime/submission_handlers"
            }
        }))])
    }

    fn effect_replay_control_user_message() -> Message {
        Message::user_parts(vec![ContentPart::structured(serde_json::json!({
            "checkpoint_id": "effect_replay_call-1",
            "checkpoint_type": "effect_replay_confirmation",
            "choice": "approve",
            "__alan_internal_control": {
                "kind": "effect_replay_confirmation",
                "version": 1,
                "source": "runtime/submission_handlers"
            }
        }))])
    }

    #[test]
    fn test_messages_for_prompt_includes_summary() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "hello"));
        ctx.set_summary("short summary".to_string());

        let messages = ctx.messages_for_prompt();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role(), MessageRole::Context);
        assert!(messages[0].text_content().contains(SUMMARY_PREFIX));
        assert!(messages[0].text_content().contains("short summary"));
        assert_eq!(messages[1].role(), MessageRole::User);
    }

    #[test]
    fn test_apply_context_items_computes_baseline_and_delta() {
        let mut ctx = Tape::new();

        let delta = ctx.apply_context_items(vec![item("a", "alpha"), item("b", "beta")]);
        assert!(delta.changed);
        assert_eq!(delta.revision, 1);
        assert_eq!(delta.added_ids, vec!["a", "b"]);
        assert!(delta.updated_ids.is_empty());
        assert!(delta.removed_ids.is_empty());
        assert_eq!(ctx.context_revision(), 1);

        let unchanged = ctx.apply_context_items(vec![item("a", "alpha"), item("b", "beta")]);
        assert!(!unchanged.changed);
        assert_eq!(unchanged.revision, 1);
        assert_eq!(ctx.context_revision(), 1);
    }

    #[test]
    fn test_apply_context_items_detects_updates_removals_and_reorder() {
        let mut ctx = Tape::new();
        ctx.apply_context_items(vec![
            item("a", "alpha"),
            item("b", "beta"),
            item("c", "gamma"),
        ]);

        let delta = ctx.apply_context_items(vec![item("b", "beta"), item("a", "alpha2")]);
        assert!(delta.changed);
        assert_eq!(delta.revision, 2);
        assert_eq!(delta.updated_ids, vec!["a"]);
        assert_eq!(delta.removed_ids, vec!["c"]);
        assert!(delta.added_ids.is_empty());

        let reorder_only = ctx.apply_context_items(vec![item("a", "alpha2"), item("b", "beta")]);
        assert!(reorder_only.changed);
        assert!(reorder_only.reordered);
        assert!(reorder_only.added_ids.is_empty());
        assert!(reorder_only.updated_ids.is_empty());
        assert!(reorder_only.removed_ids.is_empty());
    }

    #[test]
    fn test_apply_context_items_detects_content_change_with_stale_fingerprint() {
        let mut ctx = Tape::new();
        let original = item("a", "alpha");
        let stale_fingerprint = original.fingerprint.clone();
        ctx.apply_context_items(vec![original]);

        let delta = ctx.apply_context_items(vec![ContextItem {
            id: "a".to_string(),
            kind: "test".to_string(),
            title: "Title a".to_string(),
            content: "beta".to_string(),
            fingerprint: stale_fingerprint,
        }]);

        assert!(delta.changed);
        assert_eq!(delta.updated_ids, vec!["a"]);
        assert_eq!(
            ctx.context_items()[0].fingerprint,
            fingerprint_context("test", "Title a", "beta")
        );
    }

    #[test]
    fn test_prompt_view_exposes_reference_context_snapshot_metadata() {
        let mut ctx = Tape::new();
        let delta = ctx.apply_context_items(vec![item("ctx_1", "important background")]);
        assert!(delta.changed);
        ctx.push(msg(MessageRole::User, "hello"));

        let view = ctx.prompt_view();
        assert_eq!(view.reference_context.item_count, 1);
        assert_eq!(view.reference_context.revision, 1);
        assert!(view.reference_context.delta.changed);
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[0].role(), MessageRole::Context);
        assert_eq!(view.messages[1].role(), MessageRole::User);
    }

    #[test]
    fn test_compact_keeps_complete_latest_user_turn_span_and_sets_summary() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "u1"));
        ctx.push(msg(MessageRole::Assistant, "a1"));
        ctx.push(msg(MessageRole::Tool, "tool1"));
        ctx.push(msg(MessageRole::User, "u2"));
        ctx.push(msg(MessageRole::Assistant, "a2"));
        ctx.push(msg(MessageRole::Tool, "tool2"));

        ctx.compact("summary".to_string(), 3);
        let messages = ctx.messages_for_prompt();
        assert_eq!(messages.len(), 4);
        assert!(messages[0].text_content().contains("summary"));
        assert_eq!(messages[1].text_content(), "u2");
        assert_eq!(messages[2].text_content(), "a2");
        assert_eq!(messages[3].text_content(), "tool2");
    }

    #[test]
    fn test_semantic_message_spans_treat_control_preamble_as_control() {
        let spans = semantic_message_spans(&[
            msg(MessageRole::Assistant, "assistant preamble"),
            msg(MessageRole::Tool, "tool preamble"),
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
        ]);

        assert_eq!(
            spans,
            vec![
                MessageSpan {
                    start: 0,
                    end: 2,
                    kind: SpanKind::Control,
                },
                MessageSpan {
                    start: 2,
                    end: 4,
                    kind: SpanKind::UserTurn,
                },
            ]
        );
    }

    #[test]
    fn test_semantic_message_spans_do_not_start_new_turn_for_control_user_messages() {
        let spans = semantic_message_spans(&[
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            control_user_message(),
            msg(MessageRole::Assistant, "a2"),
            msg(MessageRole::User, "u2"),
        ]);

        assert_eq!(
            spans,
            vec![
                MessageSpan {
                    start: 0,
                    end: 4,
                    kind: SpanKind::UserTurn,
                },
                MessageSpan {
                    start: 4,
                    end: 5,
                    kind: SpanKind::UserTurn,
                },
            ]
        );
    }

    #[test]
    fn test_semantic_message_spans_do_not_start_new_turn_for_effect_replay_controls() {
        let spans = semantic_message_spans(&[
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            effect_replay_control_user_message(),
            msg(MessageRole::Assistant, "a2"),
            msg(MessageRole::User, "u2"),
        ]);

        assert_eq!(
            spans,
            vec![
                MessageSpan {
                    start: 0,
                    end: 4,
                    kind: SpanKind::UserTurn,
                },
                MessageSpan {
                    start: 4,
                    end: 5,
                    kind: SpanKind::UserTurn,
                },
            ]
        );
    }

    #[test]
    fn test_compact_preserves_reference_context_summary_message_order() {
        let mut ctx = Tape::new();
        ctx.apply_context_items(vec![item("ctx-1", "workspace context")]);
        ctx.push(msg(MessageRole::User, "u1"));
        ctx.push(msg(MessageRole::Assistant, "a1"));
        ctx.push(msg(MessageRole::User, "u2"));
        ctx.push(msg(MessageRole::Assistant, "a2"));

        ctx.compact("summary".to_string(), 2);

        let prompt = ctx.messages_for_prompt();
        assert_eq!(prompt[0].role(), MessageRole::Context);
        assert!(prompt[0].text_content().contains("workspace context"));
        assert_eq!(prompt[1].role(), MessageRole::Context);
        assert!(prompt[1].text_content().contains("summary"));
        assert_eq!(prompt[2].text_content(), "u2");
        assert_eq!(prompt[3].text_content(), "a2");
    }

    #[test]
    fn test_compact_reduces_estimated_prompt_tokens_with_semantic_window() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "u1"));
        ctx.push(msg(MessageRole::Assistant, "a1"));
        ctx.push(msg(MessageRole::Tool, &"log line\n".repeat(200)));
        ctx.push(msg(MessageRole::User, "u2"));
        ctx.push(msg(MessageRole::Assistant, "a2"));

        let before = ctx.estimated_prompt_tokens();
        ctx.compact("short summary".to_string(), 1);
        let after = ctx.estimated_prompt_tokens();

        assert!(after < before);
    }

    #[test]
    fn test_compaction_retention_start_uses_message_budget_not_span_count() {
        let messages = vec![
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::Tool, "tool1"),
            msg(MessageRole::Assistant, "a1b"),
            msg(MessageRole::Tool, "tool1b"),
            msg(MessageRole::User, "u2"),
            msg(MessageRole::Assistant, "a2"),
            msg(MessageRole::Tool, "tool2"),
            msg(MessageRole::Assistant, "a2b"),
            msg(MessageRole::Tool, "tool2b"),
        ];

        assert_eq!(compaction_retention_start(&messages, 6), 5);
    }

    #[test]
    fn test_compaction_retention_start_falls_back_when_latest_user_turn_exceeds_budget() {
        let messages = vec![
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::Tool, "tool1"),
            msg(MessageRole::Assistant, "a1b"),
            msg(MessageRole::Tool, "tool1b"),
            msg(MessageRole::User, "u2"),
            msg(MessageRole::Assistant, "a2"),
            msg(MessageRole::Tool, "tool2"),
            msg(MessageRole::Assistant, "a2b"),
            msg(MessageRole::Tool, "tool2b"),
        ];

        assert_eq!(compaction_retention_start(&messages, 4), 6);
    }

    #[test]
    fn test_compaction_retention_start_falls_back_for_single_large_span() {
        let messages = vec![
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::Tool, "tool1"),
            msg(MessageRole::Assistant, "a1b"),
            msg(MessageRole::Tool, "tool1b"),
        ];

        assert_eq!(compaction_retention_start(&messages, 2), 3);
    }

    #[test]
    fn test_compaction_retention_start_falls_back_for_large_recent_span_after_control_preamble() {
        let messages = vec![
            msg(MessageRole::Assistant, "assistant preamble"),
            msg(MessageRole::Tool, "tool preamble"),
            msg(MessageRole::User, "u1"),
            msg(MessageRole::Assistant, "a1"),
            msg(MessageRole::Tool, "tool1"),
            msg(MessageRole::Assistant, "a1b"),
            msg(MessageRole::Tool, "tool1b"),
        ];

        assert_eq!(compaction_retention_start(&messages, 2), 5);
    }

    #[test]
    fn test_compaction_retention_start_preserves_assistant_tool_pairing_in_raw_tail_fallback() {
        let messages = vec![
            msg(MessageRole::User, "u1"),
            assistant_with_tool_request("call_1", "lookup"),
            Message::tool_text("call_1", "tool result"),
        ];

        assert_eq!(compaction_retention_start(&messages, 1), 1);
    }

    #[test]
    fn test_compact_keeps_entire_trailing_tool_block_when_budget_is_smaller_than_block() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "u1"));
        ctx.push(Message::assistant_with_tools(
            "",
            vec![
                ToolRequest {
                    id: "call_1".to_string(),
                    name: "lookup".to_string(),
                    arguments: serde_json::json!({}),
                },
                ToolRequest {
                    id: "call_2".to_string(),
                    name: "lookup".to_string(),
                    arguments: serde_json::json!({}),
                },
            ],
        ));
        ctx.push(Message::tool_text("call_1", "tool result 1"));
        ctx.push(Message::tool_text("call_2", "tool result 2"));

        ctx.compact("summary".to_string(), 1);

        assert_eq!(ctx.summary(), Some("summary"));
        assert_eq!(ctx.messages().len(), 3);
        assert!(matches!(
            &ctx.messages()[0],
            Message::Assistant { tool_requests, .. } if !tool_requests.is_empty()
        ));
        assert!(matches!(&ctx.messages()[1], Message::Tool { .. }));
        assert!(matches!(&ctx.messages()[2], Message::Tool { .. }));
    }

    #[test]
    fn test_clear_resets_messages_summary_and_reference_context() {
        let mut ctx = Tape::new();
        ctx.apply_context_items(vec![item("x", "ctx")]);
        ctx.push(msg(MessageRole::User, "hello"));
        ctx.set_summary("summary".to_string());
        ctx.clear();

        let messages = ctx.messages_for_prompt();
        assert!(messages.is_empty());
        assert_eq!(ctx.context_revision(), 0);
        assert!(ctx.context_items().is_empty());
    }

    #[test]
    fn test_clear_summary_preserves_messages() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "hello"));
        ctx.set_summary("summary".to_string());
        ctx.clear_summary();

        let messages = ctx.messages_for_prompt();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text_content(), "hello");
    }

    #[test]
    fn test_replace_messages() {
        let mut ctx = Tape::new();
        ctx.push(msg(MessageRole::User, "old"));
        ctx.replace(vec![msg(MessageRole::Assistant, "new")]);
        let messages = ctx.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role(), MessageRole::Assistant);
    }

    #[test]
    fn test_context_items_render_before_messages() {
        let mut ctx = Tape::new();
        ctx.apply_context_items(vec![ContextItem {
            id: "onboarding".to_string(),
            kind: "static".to_string(),
            title: "Onboarding".to_string(),
            content: "Follow the steps".to_string(),
            fingerprint: fingerprint_context("static", "Onboarding", "Follow the steps"),
        }]);
        ctx.push(msg(MessageRole::User, "hello"));

        let messages = ctx.messages_for_prompt();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role(), MessageRole::Context);
        assert!(messages[0].text_content().contains("Onboarding"));
        assert_eq!(messages[1].role(), MessageRole::User);
    }

    #[test]
    fn test_estimated_prompt_tokens_includes_summary_and_context_items() {
        let mut ctx = Tape::new();
        ctx.apply_context_items(vec![ContextItem {
            id: "ctx_1".to_string(),
            kind: "domain".to_string(),
            title: "Domain".to_string(),
            content: "Important background".to_string(),
            fingerprint: fingerprint_context("domain", "Domain", "Important background"),
        }]);
        ctx.push(msg(MessageRole::User, "hello world"));
        ctx.set_summary("previous summary".to_string());

        let estimated = ctx.estimated_prompt_tokens();
        assert!(estimated > 0);

        let without_summary = {
            let mut clone = ctx.clone();
            clone.clear_summary();
            clone.estimated_prompt_tokens()
        };
        assert!(
            estimated > without_summary,
            "summary content should contribute to token estimate"
        );
    }
}
#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn test_message_role_serialization() {
        let roles = vec![
            (MessageRole::System, "\"system\""),
            (MessageRole::Context, "\"context\""),
            (MessageRole::User, "\"user\""),
            (MessageRole::Assistant, "\"assistant\""),
            (MessageRole::Tool, "\"tool\""),
        ];

        for (role, expected) in roles {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, expected);

            let deserialized: MessageRole = serde_json::from_str(expected).unwrap();
            assert!(std::mem::discriminant(&deserialized) == std::mem::discriminant(&role));
        }
    }

    #[test]
    fn test_message_serialization_user() {
        let message = Message::user("Hello");

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("user"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text_content(), "Hello");
        assert_eq!(deserialized.role(), MessageRole::User);
    }

    #[test]
    fn test_message_serialization_tool() {
        let message = Message::Tool {
            responses: vec![ToolResponse {
                id: "call_1".to_string(),
                content: vec![ContentPart::structured(
                    serde_json::json!({"result": "found"}),
                )],
            }],
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("found"));
        assert!(json.contains("tool"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role(), MessageRole::Tool);
        assert_eq!(deserialized.tool_responses().len(), 1);
        assert_eq!(deserialized.tool_responses()[0].id, "call_1");
    }

    #[test]
    fn test_message_assistant_with_tool_requests() {
        let message = Message::assistant_with_tools(
            "Let me search for that.",
            vec![ToolRequest {
                id: "call_1".to_string(),
                name: "web_search".to_string(),
                arguments: serde_json::json!({"query": "rust"}),
            }],
        );

        assert_eq!(message.role(), MessageRole::Assistant);
        assert_eq!(message.text_content(), "Let me search for that.");
        assert_eq!(message.tool_requests().len(), 1);
        assert_eq!(message.tool_requests()[0].name, "web_search");

        // Round-trip serialization
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tool_requests().len(), 1);
    }

    #[test]
    fn test_content_part_constructors() {
        let text = ContentPart::text("hello");
        assert_eq!(text.as_text(), Some("hello"));

        let thinking = ContentPart::thinking("reasoning...");
        assert_eq!(thinking.as_text(), Some("reasoning..."));

        let structured = ContentPart::structured(serde_json::json!({"key": "value"}));
        assert!(structured.as_text().is_none());
    }

    #[test]
    fn test_tool_response_text_content() {
        let resp = ToolResponse {
            id: "call_1".to_string(),
            content: vec![ContentPart::text("part1"), ContentPart::text("part2")],
        };
        assert_eq!(resp.text_content(), "part1part2");
    }

    #[test]
    fn test_message_is_predicates() {
        assert!(Message::user("hi").is_user());
        assert!(Message::assistant("hi").is_assistant());
        assert!(Message::system("hi").is_system());
        assert!(Message::context("hi").is_context());
        assert!(Message::Tool { responses: vec![] }.is_tool());
    }
}
