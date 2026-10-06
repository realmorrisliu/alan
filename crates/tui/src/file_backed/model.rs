//! Read-only consumer of Process-owned safe model observations.
use super::FileBackedApp;
#[cfg(test)]
use super::FileBackedEvent;
use alan_agent_protocol::{UiModelBinding, UiModelSnapshot};

#[derive(Clone, Default)]
pub(super) struct ModelProjection {
    pub owner: String,
    pub snapshot: Option<UiModelSnapshot>,
    revision: u64,
}
impl ModelProjection {
    pub fn apply(&mut self, owner: &str, snapshot: Option<UiModelSnapshot>) {
        if self.owner != owner {
            *self = Self {
                owner: owner.into(),
                ..Self::default()
            };
        }
        let expected = owner.replace("/agent/", "/proc/");
        let Some(next) = snapshot.filter(|s| {
            s.is_valid() && (s.process_path == expected || (!s.known && s.process_path.is_empty()))
        }) else {
            self.snapshot = None;
            return;
        };
        if next.known && next.publication_version < self.revision {
            return;
        }
        if next.known {
            self.revision = next.publication_version;
        }
        self.snapshot = Some(next);
    }
    pub fn known(&self) -> Option<&UiModelSnapshot> {
        self.snapshot.as_ref().filter(|s| s.known)
    }
    pub fn header(&self) -> String {
        self.header_controls(true)
    }
    pub fn header_controls(&self, controls: bool) -> String {
        let Some(s) = self.known() else {
            return "unknown".into();
        };
        if let Some(active) = &s.active {
            let mut label = format!("active {}", compact_binding(active, controls));
            if let Some(next) = &s.selected_next
                && next != active
            {
                label.push_str(&format!(" · next {}", compact_binding(next, controls)));
            }
            label
        } else {
            format!(
                "next {}",
                s.selected_next.as_ref().map_or_else(
                    || "unknown".into(),
                    |binding| compact_binding(binding, controls)
                )
            )
        }
    }
    pub fn status(&self) -> String {
        let Some(s) = self.known() else {
            return "model observation unknown or unavailable".into();
        };
        let mut text = format!(
            "selected-next: {} · active: {}",
            binding_label(s.selected_next.as_ref()),
            binding_label(s.active.as_ref())
        );
        text.push_str(match &s.catalog {
            None => " · catalog: unknown or unavailable",
            Some(catalog) if catalog.models.is_empty() => " · catalog: empty",
            Some(_) => " · catalog: available",
        });
        if s.admitted.is_empty() {
            text.push_str(" · admitted: none");
        }
        for admitted in &s.admitted {
            text.push_str(&format!(
                " · admitted {}: {}",
                safe(&admitted.submission_id),
                binding_label(admitted.binding.as_ref())
            ));
        }
        text
    }
}
impl FileBackedApp {
    pub(super) fn invalidate_model_owner(&mut self) {
        let owner = self.model.owner.clone();
        self.lose_model_receipts(&owner);
        self.model = ModelProjection::default();
        self.reconcile_model_chooser();
    }
}

pub(super) fn safe(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}
fn compact_binding(binding: &UiModelBinding, controls: bool) -> String {
    if !controls {
        return safe(&binding.model);
    }
    format!(
        "{} · {}",
        safe(&binding.model),
        binding
            .reasoning
            .effort
            .map_or_else(|| "reasoning unknown".into(), |effort| effort.to_string())
    )
}
fn binding_label(binding: Option<&UiModelBinding>) -> String {
    binding.map_or_else(
        || "unknown".into(),
        |b| {
            format!(
                "{} / {} / {} · reasoning {} · source {:?}",
                safe(&b.profile),
                safe(&b.provider),
                safe(&b.model),
                b.reasoning
                    .effort
                    .map_or_else(|| "unknown".into(), |e| e.to_string()),
                b.control_source
            )
        },
    )
}
pub(super) async fn read_model(shell: &alan_shell::Shell, owner: &str) -> Option<UiModelSnapshot> {
    let path = format!("{owner}/machine/ui/models");
    // Reuse the existing bounded descriptor reader; never cat an arbitrary document.
    let text = super::action_detail_io::reference::document_with_budget(shell, &path, 1048576)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}
pub(super) fn start_selection(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    owner: String,
    id: String,
    op: alan_agent_protocol::Op,
    jobs: &mut tokio::task::JoinSet<()>,
    tx: &tokio::sync::mpsc::Sender<super::FileBackedEvent>,
) {
    if !selection_ready(app, &owner, &id) {
        return;
    }
    let shell = shell.clone();
    let tx = tx.clone();
    jobs.spawn(async move {
        let (success, owner_current) = send_selection(&shell, &owner, &id, op).await;
        let _ = tx
            .send(super::FileBackedEvent::ModelSelectionWritten {
                owner,
                id,
                success,
                owner_current,
            })
            .await;
    });
}

fn selection_ready(app: &mut FileBackedApp, owner: &str, id: &str) -> bool {
    if app.model_chooser.pending.as_ref() != Some(&(owner.into(), id.into())) {
        return false;
    }
    if app.model.owner != owner || app.queue.owner != owner {
        app.lose_model_receipts(owner);
        app.invalidate_model_owner();
        return false;
    }
    true
}

async fn send_selection(
    shell: &alan_shell::Shell,
    owner: &str,
    id: &str,
    op: alan_agent_protocol::Op,
) -> (bool, bool) {
    let alan_agent_protocol::Op::SelectModel { model } = op else {
        return (true, true);
    };
    let current = || async {
        super::current_root_agent_pid(shell)
            .await
            .ok()
            .flatten()
            .is_some_and(|pid| owner == format!("/agent/{pid}"))
    };
    if !current().await {
        return (false, false);
    }
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        super::file_surface::write_machine_ctl(shell, owner, &format!("select-model {id} {model}")),
    )
    .await;
    (matches!(result, Ok(Ok(()))), current().await)
}

pub(super) fn finish_selection(
    app: &mut FileBackedApp,
    owner: &str,
    id: &str,
    success: bool,
    owner_current: bool,
) {
    if app.model_chooser.pending.as_ref() != Some(&(owner.into(), id.into())) {
        return;
    }
    if !success || !owner_current {
        app.lose_model_receipts(owner);
        if !owner_current {
            app.invalidate_model_owner();
        }
    }
}

#[cfg(test)]
pub(super) async fn write_selection(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    owner: &str,
    id: &str,
    op: alan_agent_protocol::Op,
) {
    if !selection_ready(app, owner, id) {
        return;
    }
    let (success, owner_current) = send_selection(shell, owner, id, op).await;
    finish_selection(app, owner, id, success, owner_current);
}
#[cfg(test)]
pub(super) async fn dispatch_model_event(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    event: FileBackedEvent,
) {
    match event {
        FileBackedEvent::ModelChanged { owner } if owner == app.model.owner => {
            app.model.apply(&owner, read_model(shell, &owner).await);
            app.reconcile_model_chooser();
        }
        FileBackedEvent::ModelUnavailable { owner } if owner == app.model.owner => {
            app.model.apply(&owner, None);
            app.reconcile_model_chooser();
        }
        _ => {}
    }
}
