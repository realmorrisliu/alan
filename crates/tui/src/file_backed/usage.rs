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
                || u.context_tokens > 0
                || u.session_input_tokens > 0
                || u.session_output_tokens > 0
        })
    }

    pub fn ctx_pct(&self) -> Option<u64> {
        let u = self.active_snapshot()?;
        let ctx = ctx_tokens(u);
        (u.context_window > 0 && ctx > 0).then(|| ctx_pct(ctx, u.context_window))
    }

    pub fn ctx_segment(&self, detailed: bool) -> Option<String> {
        let u = self.active_snapshot()?;
        let ctx = ctx_tokens(u);
        if u.context_window > 0 && ctx > 0 {
            let pct = ctx_pct(ctx, u.context_window);
            if detailed {
                Some(format!(
                    "ctx {pct}% ({}/{})",
                    format_compact_tokens(ctx),
                    format_compact_tokens(u.context_window)
                ))
            } else {
                Some(format!("ctx {pct}%"))
            }
        } else {
            let total = u.session_input_tokens + u.session_output_tokens;
            Some(format!("{} tok", format_compact_tokens(total)))
        }
    }

    pub fn cache_hit_segment(&self) -> Option<String> {
        let u = self.active_snapshot()?;
        let (cached, input) = if u.input_tokens > 0 {
            (u.cached_input_tokens, u.input_tokens)
        } else {
            (u.session_cached_input_tokens, u.session_input_tokens)
        };
        cache_hit_pct(cached, input).map(|pct| format!("{pct}% hit"))
    }

    pub fn cost_segment(&self) -> Option<String> {
        let u = self.active_snapshot()?;
        u.session_cost_usd
            .or(u.cost_usd)
            .filter(|&cost| cost > 0.0)
            .map(|cost| format!("${cost:.4}"))
    }

    pub fn prompt_label(&self) -> Option<String> {
        self.prompt_parts(true)
    }

    #[cfg(test)]
    pub fn short_prompt_label(&self) -> Option<String> {
        self.prompt_parts(false)
    }

    fn prompt_parts(&self, include_details: bool) -> Option<String> {
        let mut parts = vec![self.ctx_segment(include_details)?];
        if include_details {
            if let Some(hit) = self.cache_hit_segment() {
                parts.push(hit);
            }
            if let Some(cost) = self.cost_segment() {
                parts.push(cost);
            }
        }
        Some(parts.join(" · "))
    }

    pub fn status(&self) -> Option<String> {
        let u = self.active_snapshot()?;
        let mut text = format!("usage: turn in {} out {}", u.input_tokens, u.output_tokens);
        append_cached_status(&mut text, u.cached_input_tokens, u.input_tokens);
        text.push_str(&format!(
            " · session in {} out {}",
            u.session_input_tokens, u.session_output_tokens
        ));
        append_cached_status(
            &mut text,
            u.session_cached_input_tokens,
            u.session_input_tokens,
        );
        if let Some(reasoning) = u.reasoning_tokens.filter(|&r| r > 0) {
            text.push_str(&format!(" · reasoning {reasoning}"));
        }
        if u.context_window > 0 {
            let ctx = ctx_tokens(u);
            if ctx > 0 {
                let pct = ctx_pct(ctx, u.context_window);
                text.push_str(&format!(" · ctx {pct}% ({ctx}/{})", u.context_window));
            } else {
                text.push_str(&format!(" · ctx {}", u.context_window));
            }
        }
        if let Some(cost) = u.session_cost_usd.or(u.cost_usd) {
            text.push_str(&format!(" · cost ${cost:.4}"));
        }
        Some(text)
    }
}

fn ctx_tokens(u: &UiUsageSnapshot) -> u64 {
    if u.context_tokens > 0 {
        u.context_tokens
    } else {
        u.input_tokens
    }
}

fn ctx_pct(ctx: u64, context_window: u64) -> u64 {
    ((ctx as f64 / context_window as f64) * 100.0)
        .round()
        .clamp(0.0, 999.0) as u64
}

fn cache_hit_pct(cached: Option<u64>, input: u64) -> Option<u64> {
    let cached = cached.filter(|&c| c > 0)?;
    (input > 0).then(|| (((cached as u128) * 100) / (input as u128)).min(100) as u64)
}

fn append_cached_status(text: &mut String, cached: Option<u64>, input: u64) {
    if let Some(cached) = cached.filter(|&c| c > 0) {
        match cache_hit_pct(Some(cached), input) {
            Some(pct) => text.push_str(&format!(" (cached {cached}, {pct}%)")),
            None => text.push_str(&format!(" (cached {cached})")),
        }
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
