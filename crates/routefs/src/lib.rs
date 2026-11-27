//! alan-routefs — typed message routing as a crash-isolated aP file server.
//!
//! A sender writes one typed message document to `send` and commits it by
//! clunking the fid. Routefs matches the complete message against plain rule
//! files in deterministic name order, delivers it to a destination port stream,
//! and appends the routing decision to `log`. The sender emits a result type; it
//! does not name the receiving actor.
//!
//! [`RouteFs`] executes its routing state inside a single-owner
//! [`RouteFsActor`] spawned via [`alan_ap::spawn_file_actor`] without internal
//! `tokio::sync::Mutex` locks, offloading blocking `ports/*` and `log` stream
//! reads outside the actor loop via [`alan_ap::ActorOutcome::ReadStream`].

mod actor;
mod handoff;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{
    ActorExitHandle, ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat,
    spawn_file_actor,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use actor::RouteFsActor;
use actor::{MessageDocument, rule_source, valid_name};
pub use handoff::{
    LOOP_GUARD_RULE_NAME, MAX_HANDOFF_HOPS, ROUTE_HANDOFF_VERSION, RouteHandoffKind,
    RouteHandoffMessage, RoutedRecord, workstream_handoff_rules,
};

/// Canonical `/srv` handle name for the routefs server.
pub const SRV_HANDLE: &str = "route";
/// Canonical mount path for routefs.
pub const MOUNT_PATH: &str = "/mnt/route";
/// Default port for unrouted or loop-bounded messages.
pub const DEAD_LETTER_PORT: &str = "dead-letter";
/// Default coordinator port receiving cross-workstream handoffs for Root.
pub const ROOT_PORT: &str = "root";
/// Default bounded mailbox capacity for [`RouteFs`] actor instances.
pub const DEFAULT_MAILBOX_CAP: usize = 256;

const INTERNAL_FID_BASE: u64 = 1 << 63;

/// A plain JSON rule file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSpec {
    pub version: u16,
    /// Message type to match. If omitted, any type matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    /// Substring required in the message content. If omitted, content is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
    /// Destination port name under `ports/`.
    pub port: String,
    /// Human-readable reason, retained in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RuleSpec {
    /// Rule matching a message type.
    pub fn for_type(message_type: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            version: 1,
            match_type: Some(message_type.into()),
            content_contains: None,
            port: port.into(),
            reason: None,
        }
    }

    /// Attach a content substring condition.
    pub fn with_content_contains(mut self, content: impl Into<String>) -> Self {
        self.content_contains = Some(content.into());
        self
    }

    /// Attach an inspectable reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub(crate) fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != 1 || !valid_name(&self.port) {
            return Err(ErrorCode::BadRequest);
        }
        if self.match_type.as_deref().is_some_and(str::is_empty)
            || self.content_contains.as_deref().is_some_and(str::is_empty)
        {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }

    pub(crate) fn matches(&self, message: &MessageDocument) -> bool {
        let type_matches = self
            .match_type
            .as_deref()
            .is_none_or(|expected| expected == message.message_type);
        let content_matches = self
            .content_contains
            .as_deref()
            .is_none_or(|needle| message.content.contains(needle));
        type_matches && content_matches
    }
}

/// The crash-isolated message routing file server.
///
/// Wraps a spawned [`RouteFsActor`] behind [`alan_ap::spawn_file_actor`] with
/// bounded mailbox backpressure and non-blocking `Stream` read offloading.
pub struct RouteFs {
    server: Arc<dyn FileServer>,
    exit_handle: ActorExitHandle,
    next_internal_fid: AtomicU64,
}

impl Default for RouteFs {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteFs {
    /// Create an empty routefs with the default `dead-letter` and `root` ports.
    pub fn new() -> Self {
        Self::with_mailbox_cap(DEFAULT_MAILBOX_CAP)
    }

    /// Create an empty routefs using an explicit bounded actor mailbox capacity
    /// (clamped to at least `1`).
    pub fn with_mailbox_cap(mailbox_cap: usize) -> Self {
        Self::spawn(RouteFsActor::new(), mailbox_cap).0
    }

    /// Create a `RouteFs` pre-populated with default workstream handoff rules
    /// routing `reroute_request`, `cross_stream_dependency`, and `handoff_result`
    /// to `/mnt/route/ports/root`.
    pub fn new_with_workstream_handoff_rules() -> Self {
        Self::spawn(
            RouteFsActor::new_with_workstream_handoff_rules(),
            DEFAULT_MAILBOX_CAP,
        )
        .0
    }

    /// Spawn a configured [`RouteFsActor`] with bounded `mailbox_cap`, returning
    /// the [`RouteFs`] handle alongside its [`ActorExitHandle`].
    pub fn spawn(actor: RouteFsActor, mailbox_cap: usize) -> (Self, ActorExitHandle) {
        let (server, exit_handle) = spawn_file_actor(actor, mailbox_cap);
        let fs = Self {
            server,
            exit_handle: exit_handle.clone(),
            next_internal_fid: AtomicU64::new(INTERNAL_FID_BASE),
        };
        (fs, exit_handle)
    }

    /// Return a handle for observing clean exit or panic termination of the
    /// underlying [`RouteFsActor`] task.
    pub fn exit_handle(&self) -> ActorExitHandle {
        self.exit_handle.clone()
    }

    /// Install the default inspectable workstream handoff rules routing typed
    /// cross-stream messages to `/mnt/route/ports/root`.
    pub async fn install_workstream_handoff_rules(&self) -> Result<(), ErrorCode> {
        for (name, spec) in workstream_handoff_rules() {
            self.install_rule(name, spec).await?;
        }
        Ok(())
    }

    /// Install or replace a rule through the actor mailbox.
    /// Tests and bootstrap code may use this; the rule remains readable as
    /// `rules/<name>`.
    pub async fn install_rule(
        &self,
        name: impl Into<String>,
        spec: RuleSpec,
    ) -> Result<(), ErrorCode> {
        let name = name.into();
        if !valid_name(&name) {
            return Err(ErrorCode::BadRequest);
        }
        spec.validate()?;
        let bytes = rule_source(&spec)?;

        let dir_fid = self.alloc_internal_fid();
        let rule_fid = self.alloc_internal_fid();
        self.server
            .walk(Fid::ROOT, dir_fid, &["rules".to_string()])
            .await?;

        let prepare = match self
            .server
            .create(dir_fid, rule_fid, &name, FileKind::File)
            .await
        {
            Ok(_) => Ok(()),
            Err(ErrorCode::BadRequest) => match self
                .server
                .walk(dir_fid, rule_fid, std::slice::from_ref(&name))
                .await
            {
                Ok(_) => Ok(()),
                Err(ErrorCode::NotFound) => Err(ErrorCode::BadRequest),
                Err(err) => Err(err),
            },
            Err(err) => Err(err),
        };

        if let Err(err) = prepare {
            let _ = self.server.clunk(dir_fid).await;
            return Err(err);
        }

        let write_res = async {
            self.server.open(rule_fid, OpenMode::Write).await?;
            self.server.write(rule_fid, 0, &bytes).await?;
            Ok::<(), ErrorCode>(())
        }
        .await;
        let clunk_res = self.server.clunk(rule_fid).await;
        let _ = self.server.clunk(dir_fid).await;
        write_res?;
        clunk_res
    }

    fn alloc_internal_fid(&self) -> Fid {
        let raw = self.next_internal_fid.fetch_add(1, Ordering::Relaxed);
        Fid(raw | INTERNAL_FID_BASE)
    }
}

#[async_trait]
impl FileServer for RouteFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.server.walk(fid, newfid, names).await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.server.open(fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.server.read(fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.server.write(fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.server.stat(fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.server.create(fid, newfid, name, kind).await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.server.remove(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.server.clunk(fid).await
    }
}
