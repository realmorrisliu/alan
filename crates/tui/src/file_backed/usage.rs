//! Read-only consumer of Process-owned token and cost observations.
use alan_agent_protocol::UiUsageSnapshot;

#[derive(Clone, Default)]
pub(super) struct UsageProjection {
    pub owner: String,
    pub snapshot: Option<UiUsageSnapshot>,
}

impl UsageProjection {
    pub fn apply(&mut self, owner: &str, snapshot: Option<UiUsageSnapshot>) {
        if self.owner != owner {
            self.owner = owner.into();
            self.snapshot = None;
        }
        self.snapshot = snapshot.filter(UiUsageSnapshot::is_valid);
    }

    pub fn active_snapshot(&self) -> Option<&UiUsageSnapshot> {
        self.snapshot.as_ref().filter(|u| {
            u.input_tokens > 0
                || u.output_tokens > 0
                || u.session_input_tokens > 0
                || u.session_output_tokens > 0
        })
    }

    pub fn prompt_label(&self) -> Option<String> {
        let u = self.active_snapshot()?;
        let mut parts = Vec::new();
        if u.context_window > 0 && u.input_tokens > 0 {
            let pct = ((u.input_tokens as f64 / u.context_window as f64) * 100.0)
                .round()
                .clamp(0.0, 999.0) as u64;
            parts.push(format!("{pct}% ctx"));
        } else {
            let total = u.session_input_tokens + u.session_output_tokens;
            parts.push(format!("{} tok", format_compact_tokens(total)));
        }
        if let Some(cost) = u.session_cost_usd.or(u.cost_usd)
            && cost > 0.0
        {
            parts.push(format!("${cost:.4}"));
        }
        Some(parts.join(" · "))
    }

    pub fn status(&self) -> Option<String> {
        let u = self.active_snapshot()?;
        let mut text = format!(
            "usage: turn in {} out {} · session in {} out {}",
            u.input_tokens, u.output_tokens, u.session_input_tokens, u.session_output_tokens
        );
        if let Some(cached) = u.cached_input_tokens {
            text.push_str(&format!(" · cached {cached}"));
        }
        if let Some(reasoning) = u.reasoning_tokens {
            text.push_str(&format!(" · reasoning {reasoning}"));
        }
        if u.context_window > 0 {
            text.push_str(&format!(" · ctx {}", u.context_window));
        }
        if let Some(cost) = u.session_cost_usd.or(u.cost_usd) {
            text.push_str(&format!(" · cost ${cost:.4}"));
        }
        Some(text)
    }
}

fn format_compact_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub(super) async fn read_usage(shell: &alan_shell::Shell, owner: &str) -> Option<UiUsageSnapshot> {
    let path = format!("{owner}/machine/ui/usage");
    let text = super::action_detail_io::reference::document_with_budget(shell, &path, 65536)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}
