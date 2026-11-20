use super::*;

#[derive(Clone, Default)]
pub(in crate::file_backed) struct ModelChooser {
    pub active: bool,
    pub index: usize,
    pub catalog: Option<alan_agent_protocol::UiModelCatalog>,
    pub uncertain: Option<(String, String)>,
    pub pending: Option<(String, String)>,
    pub query: String,
    pub filtered: Vec<usize>,
    pub effort_index: usize,
    pub preferred_effort: Option<alan_agent_protocol::ReasoningEffort>,
    pub selected_model: Option<String>,
}

impl FileBackedApp {
    pub(super) fn model_submit_command(&self) -> bool {
        self.input_intent == InputIntent::Agent
            && (self.composer.text().trim() == "/model"
                || self.composer.text().trim().starts_with("/model "))
    }

    pub(super) fn model_status_command(&mut self) -> Option<FileBackedAction> {
        let project = self.project.as_ref().map_or_else(
            || "project access: no retained grant".to_string(),
            |p| {
                format!(
                    "project: {} · access: {} · mount: {}",
                    super::super::model::safe(&p.label),
                    p.access.label(),
                    p.namespace_path
                )
            },
        );
        let usage = self
            .usage
            .status()
            .map_or_else(String::new, |u| format!(" · {u}"));
        let git = self
            .git_status
            .as_ref()
            .map_or_else(String::new, |g| format!(" · {}", g.label()));
        self.notice = Some(
            format!(
                "cwd: {} · {project}{git} · {} · {}{usage}",
                self.namespace_cwd.display(),
                self.model.status(),
                self.queue.label()
            )
            .into(),
        );
        None
    }

    fn model_control_available(&self) -> bool {
        !self.model.owner.is_empty()
            && self.model.owner != "/agent/root"
            && self.model.owner == self.queue.owner
            && self.model.known().is_some()
            && self.model_chooser.pending.is_none()
            && self.model_chooser.uncertain.is_none()
    }

    pub(in crate::file_backed) fn model_command(
        &mut self,
        argument: &str,
    ) -> Option<FileBackedAction> {
        let arg = argument.trim();
        if arg == "profile" || arg.starts_with("profile ") {
            let current = self
                .model
                .known()
                .and_then(|s| s.selected_next.as_ref().or(s.active.as_ref()))
                .map(|b| super::super::model::safe(&b.profile))
                .unwrap_or_else(|| "default".to_string());
            self.notice = Some(
                format!(
                    "Active connection profile: {current}. To manage or switch profiles, run `alan connection list` and `alan connection default set <profile>`."
                )
                .into(),
            );
            return None;
        }
        if let Some(rest) = arg.strip_prefix("effort") {
            let value = rest.trim();
            if value.is_empty() {
                let current = self
                    .model_chooser
                    .preferred_effort
                    .or_else(|| {
                        self.model
                            .known()
                            .and_then(|s| s.selected_next.as_ref().or(s.active.as_ref()))
                            .and_then(|b| b.reasoning.effort)
                    })
                    .map_or_else(|| "default".to_string(), |e| e.to_string());
                self.notice = Some(
                    format!(
                        "Reasoning effort: {current}. Open `/model` and press Tab or ←/→ to inspect supported levels."
                    )
                    .into(),
                );
                return None;
            }
            if let Some(effort) = parse_reasoning_effort(value) {
                self.model_chooser.preferred_effort = Some(effort);
                self.notice = Some(
                    format!(
                        "Preferred reasoning effort noted: {effort} (press Tab/←/→ inside `/model` to preview supported levels)."
                    )
                    .into(),
                );
            } else {
                self.notice = Some(Notice::warning(
                    "Invalid reasoning effort; expected one of: none, minimal, low, medium, high, xhigh, max, ultra",
                ));
            }
            return None;
        }
        if !self.model_control_available() {
            self.notice = Some(
                "model selection requires known pinned Process and settled model control".into(),
            );
            return None;
        }
        let Some(catalog) = self.model.known().and_then(|s| s.catalog.as_ref()) else {
            self.notice = Some(Notice::warning(
                "Process model catalog unknown or unavailable",
            ));
            return None;
        };
        if flattened_catalog_choices(catalog).is_empty() {
            self.notice = Some("Process model catalog is empty".into());
            return None;
        }
        self.model_chooser.catalog = Some(catalog.clone());
        self.model_chooser.active = true;
        self.model_chooser.index = 0;
        self.model_chooser.effort_index = 0;
        self.model_chooser.query = arg.to_string();
        self.completion = None;
        self.refresh_model_choice();
        None
    }

    fn refresh_model_choice(&mut self) {
        let Some(snapshot) = self.model.known() else {
            self.model_chooser.filtered.clear();
            self.completion = None;
            return;
        };
        let Some(catalog) = snapshot.catalog.as_ref() else {
            self.model_chooser.filtered.clear();
            self.completion = None;
            return;
        };

        let entries = flattened_catalog_choices(catalog);
        let query = self.model_chooser.query.clone();
        let mut filtered: Vec<(i32, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let haystack = if entry.is_multi_profile {
                    format!(
                        "{} {} {}",
                        entry.choice.model,
                        entry.profile,
                        entry.provider.unwrap_or("")
                    )
                } else {
                    entry.choice.model.clone()
                };
                crate::completion::fuzzy_score(&haystack, &query).map(|score| (score, idx))
            })
            .collect();
        if !query.is_empty() {
            filtered.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        }
        self.model_chooser.filtered = filtered.iter().map(|(_, idx)| *idx).collect();
        if self.model_chooser.filtered.is_empty() {
            self.model_chooser.index = 0;
        } else {
            self.model_chooser.index = self
                .model_chooser
                .index
                .min(self.model_chooser.filtered.len() - 1);
        }

        let highlighted_effort = self.selected_effort_for_highlighted(catalog);
        let choices = self
            .model_chooser
            .filtered
            .iter()
            .enumerate()
            .map(|(visible_idx, &entry_idx)| {
                let entry = &entries[entry_idx];
                let m = entry.choice;
                let name = super::super::model::safe(&m.model);
                let detail = if !entry.is_multi_profile
                    && m.supported_reasoning_efforts.is_empty()
                    && query.is_empty()
                {
                    "selected-next only".to_string()
                } else {
                    let mut detail_parts = if entry.is_multi_profile {
                        let mut parts = vec![super::super::model::safe(entry.profile)];
                        if let Some(provider) = entry.provider
                            && !provider.trim().is_empty()
                        {
                            parts.push(super::super::model::safe(provider));
                        }
                        parts
                    } else {
                        vec![format!(
                            "next turn · profile {}",
                            super::super::model::safe(entry.profile)
                        )]
                    };
                    if !m.supported_reasoning_efforts.is_empty() {
                        let active_effort = if visible_idx == self.model_chooser.index {
                            highlighted_effort
                        } else {
                            m.default_reasoning_effort
                        };
                        let levels = m
                            .supported_reasoning_efforts
                            .iter()
                            .map(|effort| {
                                if active_effort == Some(*effort) {
                                    format!("{effort}*")
                                } else {
                                    effort.to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        detail_parts.push(format!("effort: {levels}"));
                    }
                    detail_parts.join(" · ")
                };
                CompletionCandidate::new(name, Some(detail))
            })
            .collect::<Vec<_>>();

        let selected = self.model_chooser.index;
        let has_choices = !choices.is_empty();
        self.completion = Some(CompletionState {
            kind: completion::CompletionKind::Command,
            token_start: 0,
            query: query.clone(),
            matches: choices,
            selected,
        });
        if !query.is_empty() || !has_choices {
            let count = self.model_chooser.filtered.len();
            self.notice = Some(
                format!(
                    "Filter: \"{query}\" ({count} match{}) · ↑↓ select · Tab effort · Enter apply · Esc cancel",
                    if count == 1 { "" } else { "es" }
                )
                .into(),
            );
        } else {
            self.notice = None;
        }
    }

    fn selected_effort_for_highlighted(
        &self,
        catalog: &alan_agent_protocol::UiModelCatalog,
    ) -> Option<alan_agent_protocol::ReasoningEffort> {
        let entry_idx = *self.model_chooser.filtered.get(self.model_chooser.index)?;
        let entries = flattened_catalog_choices(catalog);
        let choice = entries.get(entry_idx)?.choice;
        choice
            .supported_reasoning_efforts
            .get(self.model_chooser.effort_index)
            .copied()
            .or(choice.default_reasoning_effort)
    }

    pub(in crate::file_backed) fn model_key(
        &mut self,
        key: KeyEvent,
        _admitted: bool,
    ) -> Option<FileBackedAction> {
        self.reconcile_model_chooser();
        if !self.model_chooser.active {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc | KeyCode::Char('c') if key.code == KeyCode::Esc || ctrl => {
                self.model_chooser.active = false;
                self.model_chooser.query.clear();
                self.completion = None;
                self.notice = None;
            }
            KeyCode::Up => {
                self.model_chooser.index = self.model_chooser.index.saturating_sub(1);
                self.model_chooser.effort_index = 0;
                self.refresh_model_choice();
            }
            KeyCode::Down => {
                let count = self.model_chooser.filtered.len();
                self.model_chooser.index =
                    (self.model_chooser.index + 1).min(count.saturating_sub(1));
                self.model_chooser.effort_index = 0;
                self.refresh_model_choice();
            }
            KeyCode::Tab | KeyCode::Left | KeyCode::Right if !ctrl && !alt => {
                self.cycle_model_effort(key.code == KeyCode::Left);
            }
            KeyCode::Backspace => {
                if !self.model_chooser.query.is_empty() {
                    self.model_chooser.query.pop();
                    self.model_chooser.index = 0;
                    self.model_chooser.effort_index = 0;
                    self.refresh_model_choice();
                }
            }
            KeyCode::Char('u') if ctrl => {
                if !self.model_chooser.query.is_empty() {
                    self.model_chooser.query.clear();
                    self.model_chooser.index = 0;
                    self.model_chooser.effort_index = 0;
                    self.refresh_model_choice();
                }
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.model_chooser.query.push(ch);
                self.model_chooser.index = 0;
                self.model_chooser.effort_index = 0;
                self.refresh_model_choice();
            }
            KeyCode::Enter => {
                if !self.model_control_available() {
                    self.notice = Some("model selection blocked by admitted input".into());
                    return None;
                }
                let catalog = self.model.known()?.catalog.as_ref()?;
                let entries = flattened_catalog_choices(catalog);
                let entry_idx = *self.model_chooser.filtered.get(self.model_chooser.index)?;
                let entry = entries.get(entry_idx)?;
                let choice = entry.choice.model.clone();
                if choice.is_empty() || choice.chars().any(char::is_control) {
                    self.notice = Some("invalid catalog choice".into());
                    return None;
                }
                let selection = if entry.is_multi_profile {
                    format!("{}::{choice}", entry.profile)
                } else {
                    choice.clone()
                };
                let display_label = if entry.is_multi_profile {
                    format!("{choice} ({})", entry.profile)
                } else {
                    choice
                };
                let effort = self.selected_effort_for_highlighted(catalog);
                if let Some(effort) = effort {
                    self.model_chooser.preferred_effort = Some(effort);
                }
                let id = alan_agent_protocol::UserInputRecord::new(
                    InputIntent::Agent,
                    alan_agent_protocol::InputMode::FollowUp,
                    "",
                )
                .submission_id;
                let owner = self.model.owner.clone();
                self.model_chooser.active = false;
                self.model_chooser.query.clear();
                self.model_chooser.selected_model = Some(display_label.clone());
                self.model_chooser.pending = Some((owner.clone(), id.clone()));
                self.completion = None;
                self.notice = Some(format!("Switching next-turn model to {display_label}…").into());
                return Some(FileBackedAction::SelectModel {
                    owner,
                    id,
                    op: alan_agent_protocol::Op::SelectModel { model: selection },
                });
            }
            _ => {}
        }
        None
    }

    fn cycle_model_effort(&mut self, reverse: bool) {
        let Some(catalog) = self.model.known().and_then(|s| s.catalog.as_ref()).cloned() else {
            return;
        };
        let entries = flattened_catalog_choices(&catalog);
        let Some(&entry_idx) = self.model_chooser.filtered.get(self.model_chooser.index) else {
            return;
        };
        let Some(entry) = entries.get(entry_idx) else {
            return;
        };
        let efforts = &entry.choice.supported_reasoning_efforts;
        if efforts.is_empty() {
            self.notice =
                Some("Selected model does not advertise alternate reasoning effort levels.".into());
            return;
        }
        let len = efforts.len();
        self.model_chooser.effort_index = if reverse {
            (self.model_chooser.effort_index + len - 1) % len
        } else {
            (self.model_chooser.effort_index + 1) % len
        };
        let effort = efforts[self.model_chooser.effort_index];
        self.model_chooser.preferred_effort = Some(effort);
        self.refresh_model_choice();
        self.notice = Some(
            format!(
                "Reasoning effort highlighted: {effort} (press Enter to select model; configure profile default via `alan connection add`)"
            )
            .into(),
        );
    }

    pub(in crate::file_backed) fn reconcile_model_chooser(&mut self) {
        if self.model_chooser.active
            && self.model.known().and_then(|s| s.catalog.as_ref())
                != self.model_chooser.catalog.as_ref()
        {
            self.model_chooser.active = false;
            self.model_chooser.catalog = None;
            self.model_chooser.query.clear();
            self.completion = None;
            self.notice = Some(Notice::warning(
                "Model catalog changed or became unavailable; reopen /model to choose again.",
            ));
        }
    }

    pub(in crate::file_backed) fn lose_model_receipts(&mut self, owner: &str) {
        if self
            .model_chooser
            .pending
            .as_ref()
            .is_some_and(|(expected, _)| expected == owner)
        {
            self.model_chooser.uncertain = self.model_chooser.pending.take();
            self.notice = Some(Notice::warning(
                "model selection outcome uncertain; receipt owner unavailable; no retry",
            ));
        }
    }

    pub(in crate::file_backed) fn observe_model_receipt(
        &mut self,
        owner: &str,
        event: &UiEvent,
    ) -> bool {
        let Some((expected_owner, id)) = self
            .model_chooser
            .pending
            .as_ref()
            .or(self.model_chooser.uncertain.as_ref())
        else {
            return false;
        };
        let UiEvent::InputCompleted {
            submission_ids,
            status,
            ..
        } = event
        else {
            return false;
        };
        if owner != expected_owner || !submission_ids.contains(id) {
            return false;
        }
        let chosen = self.model_chooser.selected_model.take();
        self.notice = Some(match status {
            UiInputStatus::Completed => match chosen {
                Some(model) => format!("Model selection completed: {model} (next turn).").into(),
                None => "Model selection completed for the next turn.".into(),
            },
            UiInputStatus::Failed => Notice::warning(
                "Model selection rejected; check `alan connection test` or `/status`.",
            ),
            UiInputStatus::Cancelled => Notice::warning("Model selection cancelled."),
        });
        self.model_chooser.pending = None;
        self.model_chooser.uncertain = None;
        true
    }
}

struct FlattenedModelChoice<'a> {
    profile: &'a str,
    provider: Option<&'a str>,
    choice: &'a alan_agent_protocol::UiModelChoice,
    is_multi_profile: bool,
}

fn flattened_catalog_choices(
    catalog: &alan_agent_protocol::UiModelCatalog,
) -> Vec<FlattenedModelChoice<'_>> {
    if !catalog.profiles.is_empty() {
        let is_multi_profile = catalog.profiles.len() > 1;
        return catalog
            .profiles
            .iter()
            .flat_map(|group| {
                group.models.iter().map(move |choice| FlattenedModelChoice {
                    profile: group.profile.as_str(),
                    provider: (!group.provider.is_empty()).then_some(group.provider.as_str()),
                    choice,
                    is_multi_profile,
                })
            })
            .collect();
    }
    catalog
        .models
        .iter()
        .map(|choice| FlattenedModelChoice {
            profile: catalog.profile.as_str(),
            provider: None,
            choice,
            is_multi_profile: false,
        })
        .collect()
}

fn parse_reasoning_effort(raw: &str) -> Option<alan_agent_protocol::ReasoningEffort> {
    use alan_agent_protocol::ReasoningEffort;
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Some(ReasoningEffort::None),
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" | "extra_high" => Some(ReasoningEffort::XHigh),
        "max" => Some(ReasoningEffort::Max),
        "ultra" => Some(ReasoningEffort::Ultra),
        _ => None,
    }
}
