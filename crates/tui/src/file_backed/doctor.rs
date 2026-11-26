//! Read-only `/doctor` session diagnostics rendered from already-observed TUI/AgentFS state.
//!
//! Never performs network requests or reads host credentials/secrets from the renderer.

use alan_agent_protocol::UiActivityState;
use ratatui::text::{Line, Span};

use super::FileBackedApp;
use super::app::FileBackedAction;
use super::model::safe;
use crate::history::{HistoryCell, clean_text};

impl FileBackedApp {
    pub(in crate::file_backed) fn doctor_command(&mut self) -> Option<FileBackedAction> {
        let lines = format_doctor_lines(self);
        self.transcript.push(HistoryCell::Styled(lines));
        self.notice = Some(
            "Session diagnostics shown above. Run `alan doctor` in your shell for full store & credential diagnostics."
                .into(),
        );
        None
    }
}

fn format_doctor_lines(app: &FileBackedApp) -> Vec<Line<'static>> {
    let theme = app.theme;
    let label_style = theme.muted_style();

    let project_value = app.project.as_ref().map_or_else(
        || "none mounted".to_string(),
        |p| {
            let git = app
                .git_status
                .as_ref()
                .map_or_else(String::new, |g| format!(" · {}", g.label()));
            format!("{} ({}){git}", safe(&p.label), p.access.label())
        },
    );

    let (model_value, catalog_value) = match app.model.known() {
        Some(snapshot) => {
            let selected = snapshot.selected_next.as_ref().map_or_else(
                || "unknown".to_string(),
                |b| format!("{} / {}", safe(&b.profile), safe(&b.model)),
            );
            let active = snapshot.active.as_ref().map_or_else(
                || "idle".to_string(),
                |b| format!("{} / {}", safe(&b.profile), safe(&b.model)),
            );
            let catalog = match &snapshot.catalog {
                None => "unknown or unavailable",
                Some(c)
                    if c.models.is_empty() && c.profiles.iter().all(|p| p.models.is_empty()) =>
                {
                    "empty"
                }
                Some(_) => "available",
            };
            (
                format!("selected-next {selected} · active {active}"),
                catalog.to_string(),
            )
        }
        None => {
            let fallback = app
                .effective_model
                .as_deref()
                .map(|m| format!("{} (observation pending)", safe(m)))
                .unwrap_or_else(|| "unknown or unavailable".to_string());
            (fallback, "unknown or unavailable".to_string())
        }
    };

    let trust = app
        .trust_posture_label
        .as_deref()
        .map(safe)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "governed".to_string());
    let sandbox = app
        .sandbox_backend
        .as_deref()
        .map(safe)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let context_usage = app
        .usage
        .prompt_label()
        .unwrap_or_else(|| "no usage recorded yet".to_string());

    let activity_label = match app.activity.state {
        UiActivityState::Idle => "idle",
        UiActivityState::Running => "running",
        UiActivityState::Paused => "paused",
    };
    let activity_queue = format!("{activity_label} · {}", app.queue.label());

    let row = |label: &'static str, value: String| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {label:<12}"), label_style),
            Span::raw(clean_text(&value)),
        ])
    };

    vec![
        Line::styled("Session Diagnostics (/doctor)", theme.accent_bold()),
        row("process", safe(&app.agent_path)),
        row("project", project_value),
        row("model", model_value),
        row("catalog", catalog_value),
        row("trust", format!("{trust} · sandbox {sandbox}")),
        row("context", context_usage),
        row("activity", activity_queue),
        Line::styled(
            "  Tip: run `alan doctor` in your shell for full store, binary, and credential diagnostics.",
            label_style,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_backed::{ProjectAccess, ProjectMountReceipt};
    use alan_agent_protocol::{
        ReasoningControls, ReasoningEffort, UiModelBinding, UiModelCatalog, UiModelChoice,
        UiModelControlSource, UiModelSnapshot, UiUsageSnapshot,
    };

    #[test]
    fn doctor_slash_command_renders_readonly_diagnostics_and_shell_hint() {
        let mut app = FileBackedApp::new("/agent/42".to_string());
        app.project = Some(ProjectMountReceipt {
            grant_id: "g-1".to_string(),
            namespace_path: "/mnt/project-request-1".to_string(),
            label: "utopia".to_string(),
            access: ProjectAccess::ReadWrite,
        });
        app.trust_posture_label = Some("trust".to_string());
        app.sandbox_backend = Some("seatbelt".to_string());
        app.model.apply(
            "/agent/42",
            Some(UiModelSnapshot {
                process_path: "/proc/42".to_string(),
                known: true,
                publication_version: 1,
                selected_next: Some(UiModelBinding {
                    profile: "openai-main".to_string(),
                    provider: "openai_responses".to_string(),
                    model: "gpt-5.4".to_string(),
                    reasoning: ReasoningControls {
                        effort: Some(ReasoningEffort::Medium),
                    },
                    control_source: UiModelControlSource::ProviderDefault,
                }),
                active: None,
                catalog: Some(UiModelCatalog {
                    profile: "openai-main".to_string(),
                    models: vec![UiModelChoice {
                        model: "gpt-5.4".to_string(),
                        supported_reasoning_efforts: Vec::new(),
                        default_reasoning_effort: Some(ReasoningEffort::Medium),
                    }],
                    profiles: Vec::new(),
                }),
                admitted: Vec::new(),
                ..UiModelSnapshot::default()
            }),
        );
        app.usage.apply(
            "/agent/42",
            Some(UiUsageSnapshot {
                version: alan_agent_protocol::UI_SURFACE_VERSION,
                input_tokens: 12_000,
                output_tokens: 1_500,
                session_input_tokens: 24_000,
                session_output_tokens: 3_000,
                context_window: 128_000,
                ..UiUsageSnapshot::default()
            }),
        );

        let action = app.handle_command("/doctor");
        assert!(action.is_none(), "/doctor must be strictly read-only");
        assert_eq!(app.transcript.len(), 1);
        let rendered = match &app.transcript[0] {
            HistoryCell::Styled(lines) => lines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            other => panic!("expected styled transcript cell, got {other:?}"),
        };

        assert!(
            rendered.contains("Session Diagnostics (/doctor)"),
            "{rendered}"
        );
        assert!(rendered.contains("/agent/42"), "{rendered}");
        assert!(rendered.contains("utopia (read-write)"), "{rendered}");
        assert!(
            !rendered.contains("/mnt/project-request-1"),
            "project line should display label rather than raw internal mount path: {rendered}"
        );
        assert!(rendered.contains("openai-main / gpt-5.4"), "{rendered}");
        assert!(rendered.contains("available"), "{rendered}");
        assert!(rendered.contains("trust"), "{rendered}");
        assert!(rendered.contains("seatbelt"), "{rendered}");
        assert!(rendered.contains("ctx 9%"), "{rendered}");
        assert!(rendered.contains("idle"), "{rendered}");
        assert!(rendered.contains("alan doctor"), "{rendered}");
        let notice = app.notice.as_deref().unwrap_or("");
        assert!(notice.contains("alan doctor"), "{notice}");
    }

    #[test]
    fn doctor_is_registered_in_completions_and_help() {
        let app = FileBackedApp::new("/agent/root".to_string());
        assert!(
            app.completion_sources
                .commands
                .iter()
                .any(|c| c.value == "doctor"),
            "missing /doctor in slash completion candidates"
        );
        let help = super::super::help::help_text();
        assert!(help.contains("/doctor"), "missing /doctor in /help text");
    }
}
