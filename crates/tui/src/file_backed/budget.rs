//! Read-only consumer of Process-owned live task budget observations.
use alan_agent_protocol::UiBudgetSnapshot;

#[derive(Clone, Default)]
pub(super) struct BudgetProjection {
    pub owner: String,
    pub snapshot: Option<UiBudgetSnapshot>,
}

impl BudgetProjection {
    pub fn apply(&mut self, owner: &str, snapshot: Option<UiBudgetSnapshot>) {
        if self.owner != owner {
            self.owner = owner.into();
            self.snapshot = None;
        }
        self.snapshot = snapshot.filter(UiBudgetSnapshot::is_valid);
    }

    pub fn active_snapshot(&self) -> Option<&UiBudgetSnapshot> {
        self.snapshot.as_ref().filter(|b| b.has_limits())
    }

    /// Maximum percentage consumed across elapsed time, tool calls, and model calls.
    pub fn max_ratio_pct(&self) -> Option<f64> {
        let b = self.active_snapshot()?;
        let mut max_pct: f64 = 0.0;
        let mut any = false;
        for (used, limit) in [
            (b.elapsed_secs, b.max_secs),
            (b.tool_calls, b.max_tool_calls),
            (b.model_calls, b.max_model_calls),
        ] {
            if limit > 0 {
                any = true;
                max_pct = max_pct.max((used as f64 / limit as f64) * 100.0);
            }
        }
        any.then_some(max_pct)
    }

    /// Compact single-segment budget label (e.g. `18m/30m 84/256t 43/64m`).
    /// Shown while running or when any budget dimension reaches >= 50%.
    pub fn compact_label(&self, running: bool) -> Option<String> {
        let b = self.active_snapshot()?;
        if !running && self.max_ratio_pct().is_none_or(|pct| pct < 50.0) {
            return None;
        }
        let mut parts = Vec::with_capacity(3);
        if b.max_secs > 0 {
            parts.push(format!(
                "{}/{}",
                format_compact_secs(b.elapsed_secs),
                format_compact_secs(b.max_secs)
            ));
        }
        if b.max_tool_calls > 0 {
            parts.push(format!("{}/{}t", b.tool_calls, b.max_tool_calls));
        }
        if b.max_model_calls > 0 {
            parts.push(format!("{}/{}m", b.model_calls, b.max_model_calls));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    /// Detailed multi-line summary for the Ctrl+T / Ctrl+O(s) status panel.
    pub fn detail_lines(&self) -> Vec<String> {
        let Some(b) = self.active_snapshot() else {
            return vec!["  budget: unavailable".to_string()];
        };
        let mut lines = vec![
            format!(
                "  time:   {} / {} ({:.0}%)",
                format_detailed_secs(b.elapsed_secs),
                format_detailed_secs(b.max_secs),
                ratio_pct(b.elapsed_secs, b.max_secs)
            ),
            format!(
                "  tools:  {} / {} ({:.0}%)",
                b.tool_calls,
                b.max_tool_calls,
                ratio_pct(b.tool_calls, b.max_tool_calls)
            ),
            format!(
                "  models: {} / {} ({:.0}%)",
                b.model_calls,
                b.max_model_calls,
                ratio_pct(b.model_calls, b.max_model_calls)
            ),
        ];
        if let (Some(used), Some(max)) = (b.auto_extensions_used, b.max_auto_extensions) {
            lines.push(format!("  auto-extensions: {used} / {max}"));
        }
        lines
    }
}

fn ratio_pct(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        0.0
    } else {
        (used as f64 / limit as f64) * 100.0
    }
}

pub(super) fn format_compact_secs(secs: u64) -> String {
    if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

fn format_detailed_secs(secs: u64) -> String {
    if secs >= 3600 {
        format!(
            "{}h{:02}m{:02}s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    } else if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

pub(super) async fn read_budget(
    shell: &alan_shell::Shell,
    owner: &str,
) -> Option<UiBudgetSnapshot> {
    let path = format!("{owner}/machine/ui/budget");
    let text = super::action_detail_io::reference::document_with_budget(shell, &path, 65536)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_budget(elapsed_secs: u64, tool_calls: u64, model_calls: u64) -> UiBudgetSnapshot {
        UiBudgetSnapshot {
            version: alan_agent_protocol::UI_SURFACE_VERSION,
            elapsed_secs,
            max_secs: 1800,
            tool_calls,
            max_tool_calls: 256,
            model_calls,
            max_model_calls: 64,
            auto_extensions_used: None,
            max_auto_extensions: None,
        }
    }

    #[test]
    fn compact_label_shows_when_running_or_over_half() {
        let mut proj = BudgetProjection::default();
        proj.apply("/agent/1", Some(sample_budget(10, 2, 1)));
        assert_eq!(proj.compact_label(false), None);
        assert_eq!(
            proj.compact_label(true).as_deref(),
            Some("10s/30m 2/256t 1/64m")
        );

        proj.apply("/agent/1", Some(sample_budget(1080, 84, 43)));
        assert_eq!(
            proj.compact_label(false).as_deref(),
            Some("18m/30m 84/256t 43/64m")
        );
    }

    #[test]
    fn max_ratio_pct_tracks_highest_dimension() {
        let mut proj = BudgetProjection::default();
        proj.apply("/agent/1", Some(sample_budget(100, 192, 10)));
        assert!((proj.max_ratio_pct().unwrap() - 75.0).abs() < 0.01);

        proj.apply("/agent/1", Some(sample_budget(100, 10, 58)));
        assert!(proj.max_ratio_pct().unwrap() >= 90.0);
    }
}
