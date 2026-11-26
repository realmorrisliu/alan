//! Read-only consumer of Process-owned System 1 fast-evaluator observability projections.
use alan_agent_protocol::UiSystem1Snapshot;

#[derive(Clone, Default)]
pub(super) struct System1Projection {
    pub owner: String,
    pub snapshot: Option<UiSystem1Snapshot>,
}

impl System1Projection {
    pub fn apply(&mut self, owner: &str, snapshot: Option<UiSystem1Snapshot>) {
        if self.owner != owner {
            self.owner = owner.into();
            self.snapshot = None;
        }
        self.snapshot = snapshot.filter(UiSystem1Snapshot::is_valid);
    }

    pub fn known(&self) -> Option<&UiSystem1Snapshot> {
        self.snapshot.as_ref()
    }
}

pub(super) async fn read_system1(
    shell: &alan_shell::Shell,
    owner: &str,
) -> Option<UiSystem1Snapshot> {
    let path = format!("{owner}/machine/ui/system1");
    let text = super::action_detail_io::reference::document_with_budget(shell, &path, 65536)
        .await
        .ok()?;
    serde_json::from_str(&text).ok()
}
