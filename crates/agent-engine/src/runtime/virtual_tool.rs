#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VirtualToolOutcome {
    NotVirtual,
    Continue { refresh_context: bool },
    PauseTurn,
    EndTurn,
}
