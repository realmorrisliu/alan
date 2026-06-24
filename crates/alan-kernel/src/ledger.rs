use crate::KernelEvent;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Result type returned by activity ledger operations.
pub type ActivityLedgerResult<T> = Result<T, ActivityLedgerError>;

/// Errors returned by activity ledger implementations.
#[derive(Debug)]
pub enum ActivityLedgerError {
    /// Filesystem I/O failed.
    Io(std::io::Error),
    /// Event serialization or deserialization failed.
    Json(serde_json::Error),
}

impl std::fmt::Display for ActivityLedgerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "activity ledger I/O error: {error}"),
            Self::Json(error) => write!(formatter, "activity ledger JSON error: {error}"),
        }
    }
}

impl std::error::Error for ActivityLedgerError {}

impl From<std::io::Error> for ActivityLedgerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ActivityLedgerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Append-only Kernel activity ledger.
pub trait ActivityLedger {
    /// Appends an event to the ledger.
    fn append(&mut self, event: KernelEvent) -> ActivityLedgerResult<()>;

    /// Replays recorded events without executing side effects.
    fn replay(&self) -> ActivityLedgerResult<Vec<KernelEvent>>;
}

/// In-memory activity ledger for tests and early adapters.
#[derive(Clone, Debug, Default)]
pub struct InMemoryActivityLedger {
    events: Vec<KernelEvent>,
}

impl InMemoryActivityLedger {
    /// Creates an empty in-memory ledger.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ActivityLedger for InMemoryActivityLedger {
    fn append(&mut self, event: KernelEvent) -> ActivityLedgerResult<()> {
        self.events.push(event);
        Ok(())
    }

    fn replay(&self) -> ActivityLedgerResult<Vec<KernelEvent>> {
        Ok(self.events.clone())
    }
}

/// JSONL activity ledger persisted on local disk.
#[derive(Clone, Debug)]
pub struct JsonlActivityLedger {
    path: PathBuf,
}

impl JsonlActivityLedger {
    /// Creates a JSONL ledger at the provided path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the ledger path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ActivityLedger for JsonlActivityLedger {
    fn append(&mut self, event: KernelEvent) -> ActivityLedgerResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        Ok(())
    }

    fn replay(&self) -> ActivityLedgerResult<Vec<KernelEvent>> {
        let file = match OpenOptions::new().read(true).open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str(&line)?);
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::{ActivityLedger, InMemoryActivityLedger, JsonlActivityLedger};
    use crate::{
        ActorId, CommandDescriptor, CommandInvocation, CommandRecoveryPolicy, CommandRisk,
        CommandTarget, DescriptorMetadata, EventId, InvocationHintMetadata, KernelEvent,
        KernelEventKind,
    };

    #[test]
    fn in_memory_ledger_replays_events_without_mutating_them() {
        let event = command_event(1);
        let mut ledger = InMemoryActivityLedger::new();
        ledger.append(event.clone()).expect("append event");

        let replayed = ledger.replay().expect("replay events");

        assert_eq!(replayed, vec![event]);
    }

    #[test]
    fn jsonl_ledger_appends_and_replays_events_in_order() {
        let path = std::env::temp_dir().join(format!(
            "alan-kernel-ledger-{}.jsonl",
            crate::EventId::new()
        ));
        let mut ledger = JsonlActivityLedger::new(&path);
        ledger.append(command_event(1)).expect("append first");
        ledger.append(command_event(2)).expect("append second");

        let replayed = JsonlActivityLedger::new(&path).replay().expect("replay");

        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].sequence, 1);
        assert_eq!(replayed[1].sequence, 2);

        let _ = std::fs::remove_file(path);
    }

    fn command_event(sequence: u64) -> KernelEvent {
        let actor_id = ActorId::new();
        let descriptor = CommandDescriptor {
            id: crate::CommandId::new(),
            name: "object.open".to_string(),
            target: CommandTarget::None,
            args_schema: None,
            required_capabilities: Vec::new(),
            risk: CommandRisk::Low,
            recovery: CommandRecoveryPolicy::None,
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Open object"),
        };
        KernelEvent::root(
            EventId::new(),
            sequence,
            1_772_000_000_000 + sequence,
            actor_id,
            KernelEventKind::CommandInvoked {
                invocation: CommandInvocation::from_descriptor(
                    &descriptor,
                    actor_id,
                    serde_json::json!({}),
                ),
            },
        )
    }
}
