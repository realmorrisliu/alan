//! Historical plan detail uses the existing Process-owned UI event stream.
use super::app::{FileBackedApp, FileBackedEvent};
use alan_agent_protocol::{PlanItemStatus, UiEvent, UiPlanSnapshot};
use ratatui::text::Line;

#[derive(Clone)]
pub(super) struct PlanEntry {
    pub owner: String,
    pub revision: usize,
    pub snapshot: Result<UiPlanSnapshot, String>,
    pub observed_only: bool,
}

fn limited(mut entries: Vec<PlanEntry>, owner: &str, reason: &str) -> Vec<PlanEntry> {
    entries.push(PlanEntry {
        owner: owner.into(),
        revision: 0,
        snapshot: Err(reason.into()),
        observed_only: false,
    });
    entries
}

pub(super) fn start(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    owner: &str,
) {
    app.modal.owner_path = owner.into();
    let mut owners = app.plan_owners.clone();
    if !owners.iter().any(|path| path == owner) {
        owners.push(owner.into());
    }
    let observed = app
        .transcript
        .iter()
        .filter_map(|cell| {
            if let crate::history::HistoryCell::Plan {
                snapshot,
                owner,
                revision,
            } = cell
            {
                Some(PlanEntry {
                    owner: owner.clone(),
                    revision: *revision,
                    snapshot: Ok(snapshot.clone()),
                    observed_only: true,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let shell = shell.clone();
    let tx = tx.clone();
    let path = owner.to_string();
    let generation = app.modal.generation;
    tokio::spawn(async move {
        let mut entries = Vec::new();
        for owner in owners {
            let cached = observed
                .iter()
                .filter(|entry| entry.owner == owner)
                .cloned()
                .collect::<Vec<_>>();
            match read(&shell, &owner).await {
                Ok(plans) if !plans.is_empty() => {
                    let correlated = cached.iter().all(|entry| {
                        plans.iter().any(|retained| {
                            retained.revision == entry.revision
                                && retained.snapshot == entry.snapshot
                        })
                    });
                    if !correlated {
                        entries.extend(cached);
                    }
                    entries.extend(plans);
                }
                result => {
                    if cached.is_empty() {
                        if let Err(error) = result {
                            entries.push(PlanEntry {
                                owner,
                                revision: 0,
                                snapshot: Err(error),
                                observed_only: false,
                            });
                        }
                    } else {
                        // Captured snapshots remain exact even if the stream is missing or uncorrelated.
                        entries.extend(cached);
                    }
                }
            }
        }

        let _ = tx
            .send(FileBackedEvent::PlanDetails {
                path,
                generation,
                entries,
            })
            .await;
    });
}

pub(super) async fn read(shell: &alan_shell::Shell, owner: &str) -> Result<Vec<PlanEntry>, String> {
    if owner == "/agent/root" {
        return Err("Plan history unavailable: Root Process identity is unresolved".into());
    }
    let path = format!("{owner}/machine/ui/events");
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read_events(shell, owner, &path),
    )
    .await
    .map_err(|_| "Plan history unavailable: read deadline exceeded".to_string())?
}

async fn read_events(
    shell: &alan_shell::Shell,
    owner: &str,
    path: &str,
) -> Result<Vec<PlanEntry>, String> {
    let length = shell
        .stat(path)
        .await
        .map_err(|error| format!("Plan history unavailable: {error:?}"))?
        .length;
    let budget = super::action_detail_io::reference::DISPLAY_BYTES as usize;
    let mut previous = UiPlanSnapshot::empty();
    let mut entries = Vec::new();
    let mut pending = Vec::new();
    let mut offset = 0;
    let mut plan_bytes = 0;
    while offset < length {
        let count = (length - offset).min(16_384);
        let bytes = shell
            .read_range_bounded(
                path,
                offset,
                Some(count),
                count,
                std::time::Duration::from_secs(5),
            )
            .await
            .map_err(|error| format!("Plan history unavailable: bounded read: {error:?}"))?;
        if bytes.is_empty() {
            return Err("Plan history unavailable: retained event range ended early".into());
        }
        offset += bytes.len() as u64;
        pending.extend(bytes);
        while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
            let line = pending.drain(..=end).collect::<Vec<_>>();
            if line.len() > budget {
                return Ok(limited(
                    entries,
                    owner,
                    "Plan history display bound: event exceeds 262144 bytes; original remains in AgentFS",
                ));
            }
            if line.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let event = match serde_json::from_slice::<UiEvent>(&line) {
                Ok(event) => event,
                Err(error) => {
                    return Ok(limited(
                        entries,
                        owner,
                        &format!("Plan history unavailable: invalid retained event: {error}"),
                    ));
                }
            };
            if let UiEvent::Plan { snapshot } = event {
                if snapshot != previous {
                    plan_bytes += line.len();
                    if plan_bytes > budget {
                        return Ok(limited(
                            entries,
                            owner,
                            "Plan history display bound: plan snapshots exceed 262144 bytes; original remains in AgentFS",
                        ));
                    }
                    entries.push(PlanEntry {
                        owner: owner.into(),
                        revision: entries.len() + 1,
                        snapshot: Ok(snapshot.clone()),
                        observed_only: false,
                    });
                }
                previous = snapshot;
            }
        }
        if pending.len() > budget {
            return Ok(limited(
                entries,
                owner,
                "Plan history display bound: event exceeds 262144 bytes; original remains in AgentFS",
            ));
        }
    }
    if !pending.is_empty() {
        return Ok(limited(
            entries,
            owner,
            "Plan history ends with an incomplete event; earlier complete snapshots remain available",
        ));
    }
    Ok(entries)
}

pub(super) fn rows(entry: &PlanEntry) -> Vec<Line<'static>> {
    let label = if entry.revision == 0 {
        "Plan history".into()
    } else if entry.observed_only {
        format!("Captured plan {}", entry.revision)
    } else {
        format!("Retained plan {}", entry.revision)
    };
    let mut rows = vec![Line::from(format!("{label} · {}", entry.owner))];
    match &entry.snapshot {
        Err(error) => rows.push(Line::from(format!("Plan snapshot unavailable: {error}"))),
        Ok(snapshot) => {
            if entry.observed_only {
                rows.push(Line::from(
                    "Captured UI snapshot; retained history unavailable or uncorrelated",
                ));
            }
            if let Some(explanation) = &snapshot.explanation {
                rows.extend(
                    explanation
                        .lines()
                        .map(|line| Line::from(crate::history::clean_text(line))),
                );
            }
            for item in &snapshot.items {
                let state = match item.status {
                    PlanItemStatus::Completed => "completed",
                    PlanItemStatus::InProgress => "in progress",
                    PlanItemStatus::Pending => "pending",
                };
                for (index, line) in item.content.lines().enumerate() {
                    rows.push(Line::from(if index == 0 {
                        format!("{state} · {}", crate::history::clean_text(line))
                    } else {
                        format!("  {}", crate::history::clean_text(line))
                    }));
                }
            }
            if snapshot.items.is_empty() {
                rows.push(Line::from("No plan steps"));
            }
        }
    }
    rows
}

impl FileBackedApp {
    pub(in crate::file_backed) fn apply_plan_details(
        &mut self,
        path: String,
        generation: u64,
        entries: Vec<PlanEntry>,
    ) {
        if self.modal.active
            && self.modal.plan_mode
            && self.modal.owner_path == path
            && self.modal.generation == generation
        {
            self.modal.selected = entries.len().saturating_sub(1);
            self.modal.rows = entries
                .last()
                .map_or_else(|| vec![Line::from("No retained plan snapshots")], rows);
            self.modal.plans = entries;
        }
    }
}

#[cfg(test)]
#[path = "plan_detail_tests.rs"]
mod tests;
