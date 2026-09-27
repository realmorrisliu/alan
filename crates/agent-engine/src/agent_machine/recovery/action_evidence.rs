use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use serde_json::Value;

use crate::rollout::RolloutItem;
use crate::tape::{ContentPart, Message};

/// Map only recorded Action identities; PID or Action-number reuse is not identity.
pub(super) fn rebase(items: &mut [RolloutItem], process_path: &str) -> Result<()> {
    if !items.iter().any(|item| {
        matches!(item,
            RolloutItem::Event(event) if event.event_type == "agent_action_v1"
        )
    }) {
        return Ok(());
    }
    let pid = process_path
        .strip_prefix("/proc/")
        .context("recovery Process path")?;
    let agent_path = format!("/agent/{pid}");
    let mut expired = HashMap::new();
    for item in items.iter() {
        if let RolloutItem::Event(event) = item
            && event.event_type == "agent_action_retention_v1"
        {
            let source = event.payload["agent_path"]
                .as_str()
                .context("retention Agent path")?;
            let id = event.payload["action_id"]
                .as_str()
                .context("retention Action ID")?;
            let cause = event.payload["cause"].as_str().context("retention cause")?;
            expired.insert(format!("{source}/actions/{id}/output"), cause.to_string());
        }
    }
    let mut paths = HashMap::new();
    for item in items.iter_mut() {
        if let RolloutItem::Event(event) = item
            && event.event_type == "agent_action_v1"
        {
            let source = event.payload["agent_path"]
                .as_str()
                .context("Action source Agent path")?;
            let id = event.payload["action_id"]
                .as_str()
                .context("Action source ID")?;
            let old_path = format!("{source}/actions/{id}/output");
            let new_id = format!("a{}", paths.len());
            let new_path = format!("{agent_path}/actions/{new_id}/output");
            if let Some(cause) = expired.get(&old_path) {
                // Replace the durable payload before the recovered rollout is written.
                event.payload["record"]["output"] = Value::String(
                    serde_json::json!({
                        "type": "evidence_retention_expired",
                        "reference": format!("/actions/{new_id}/output"),
                        "cause": cause,
                    })
                    .to_string(),
                );
            }
            ensure!(
                paths.insert(old_path, new_path).is_none(),
                "duplicate durable Action identity"
            );
            // Re-persist the projection identity so a second restart maps from this Process.
            event.payload["agent_path"] = Value::String(agent_path.clone());
            event.payload["action_id"] = Value::String(new_id);
        }
    }
    for item in items {
        if let RolloutItem::Event(event) = item
            && event.event_type == "agent_action_retention_v1"
        {
            let old_path = format!(
                "{}/actions/{}/output",
                event.payload["agent_path"]
                    .as_str()
                    .context("retention Agent path")?,
                event.payload["action_id"]
                    .as_str()
                    .context("retention Action ID")?
            );
            if let Some(path) = paths.get(&old_path) {
                event.payload["agent_path"] = Value::String(agent_path.clone());
                event.payload["action_id"] = Value::String(path.rsplit('/').nth(1).unwrap().into());
            }
        }
        if let RolloutItem::Message(record) = item
            && let Some(Message::Tool { responses }) = record.message.as_mut()
        {
            for response in responses {
                for part in &mut response.content {
                    match part {
                        ContentPart::Structured { data } => {
                            rebase_value(data, &paths);
                        }
                        ContentPart::Text { text } => {
                            if let Ok(mut data) = serde_json::from_str::<Value>(text)
                                && rebase_value(&mut data, &paths)
                            {
                                *text = serde_json::to_string(&data)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            record.content = record.message.as_ref().map(Message::text_content);
        }
    }
    Ok(())
}

fn rebase_value(value: &mut Value, paths: &HashMap<String, String>) -> bool {
    let mut changed = false;
    if let Value::Object(object) = value {
        let projection = object.get("type").and_then(Value::as_str) == Some("evidence_projection");
        for key in ["reference", "output_ref"] {
            if (key == "output_ref" || projection)
                && let Some(path) = object
                    .get_mut(key)
                    .and_then(|reference| reference.get_mut("path"))
                && let Some(replacement) = path.as_str().and_then(|path| paths.get(path))
            {
                *path = Value::String(replacement.clone());
                changed = true;
            }
        }
        for child in object.values_mut() {
            changed |= rebase_value(child, paths);
        }
    } else if let Value::Array(values) = value {
        for child in values {
            changed |= rebase_value(child, paths);
        }
    }
    changed
}
