//! Generation-tracked `/srv/*` service handle proxy.
//!
//! Wraps a supervised unit's active [`FileServer`] behind a stable namespace
//! mount. Each [`SupervisedHandle::bind`] increments a monotonic `generation`
//! counter, and [`SupervisedHandle::deactivate`] clears the active target.
//! Operations on fids walked or created in an earlier generation fail cleanly
//! with [`ErrorCode::Io`] rather than mutating a restarted service lifetime,
//! while fresh walks from [`Fid::ROOT`] bind transparently to the current
//! server.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat};
use anyhow::{Result, ensure};
use async_trait::async_trait;

#[derive(Clone, Copy, Debug)]
enum FidSlot {
    Reserved { generation: u64, backing_fid: Fid },
    Bound { generation: u64, backing_fid: Fid },
}

struct HandleState {
    active: Option<Arc<dyn FileServer>>,
    generation: u64,
    next_backing_fid: u64,
    fids: HashMap<Fid, FidSlot>,
}

struct ReservedChildFid<'a> {
    server: Arc<dyn FileServer>,
    base_backing_fid: Fid,
    backing_newfid: Fid,
    generation: u64,
    guard: ReservationGuard<'a>,
}

struct ReservationGuard<'a> {
    state: &'a Mutex<HandleState>,
    client_fid: Fid,
    generation: u64,
    backing_fid: Fid,
    armed: bool,
}

impl Drop for ReservationGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(
            state.fids.get(&self.client_fid),
            Some(FidSlot::Reserved {
                generation,
                backing_fid,
            }) if *generation == self.generation && *backing_fid == self.backing_fid
        ) {
            state.fids.remove(&self.client_fid);
        }
    }
}

/// A stable `/srv/*` [`FileServer`] handle whose active backing server can be
/// deactivated and rebound across supervised service restarts.
///
/// Every [`Self::bind`] advances a monotonic `generation: u64`. Client fids
/// walked or created in an earlier generation are rejected with
/// [`ErrorCode::Io`] without touching the restarted server, while fresh walks
/// from [`Fid::ROOT`] bind transparently to the new active server.
pub struct SupervisedHandle {
    state: Mutex<HandleState>,
}

impl SupervisedHandle {
    /// Create an unbound handle at generation `0`.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(HandleState {
                active: None,
                generation: 0,
                next_backing_fid: 1,
                fids: HashMap::new(),
            }),
        })
    }

    /// Create a handle immediately bound to `server` at generation `1`.
    pub fn bound(server: Arc<dyn FileServer>) -> Arc<Self> {
        let handle = Self::new();
        handle.bind(server);
        handle
    }

    /// Install `server` as the active backing implementation, advance the
    /// monotonic generation counter, and return the new generation.
    pub fn bind(&self, server: Arc<dyn FileServer>) -> u64 {
        let mut state = self.lock_state();
        state.generation = state.generation.saturating_add(1);
        state.active = Some(server);
        state.generation
    }

    /// Clear the active backing server without resetting `generation`.
    pub fn deactivate(&self) {
        let mut state = self.lock_state();
        state.active = None;
    }

    /// Return the current monotonic generation (`0` before the first bind).
    pub fn generation(&self) -> u64 {
        self.lock_state().generation
    }

    /// Return whether an active backing server is currently bound.
    pub fn is_bound(&self) -> bool {
        self.lock_state().active.is_some()
    }

    /// Execute `action` only while an active backing server is bound.
    pub async fn while_bound_async<T, F>(
        &self,
        unavailable_message: &str,
        action: impl FnOnce() -> F,
    ) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        ensure!(self.is_bound(), "{unavailable_message}");
        action().await
    }

    fn lock_state(&self) -> MutexGuard<'_, HandleState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn resolve_active_fid(&self, fid: Fid) -> Result<(Arc<dyn FileServer>, Fid, u64), ErrorCode> {
        let state = self.lock_state();
        let server = state.active.as_ref().cloned().ok_or(ErrorCode::Io)?;
        if fid == Fid::ROOT {
            return Ok((server, Fid::ROOT, state.generation));
        }
        match state.fids.get(&fid) {
            Some(FidSlot::Bound {
                generation,
                backing_fid,
            }) if *generation == state.generation => Ok((server, *backing_fid, state.generation)),
            Some(FidSlot::Bound { .. }) => Err(ErrorCode::Io),
            Some(FidSlot::Reserved { generation, .. }) if *generation == state.generation => {
                Err(ErrorCode::BadRequest)
            }
            Some(FidSlot::Reserved { .. }) => Err(ErrorCode::Io),
            None => Err(ErrorCode::NotFound),
        }
    }

    fn reserve_child_fid(&self, fid: Fid, newfid: Fid) -> Result<ReservedChildFid<'_>, ErrorCode> {
        let mut state = self.lock_state();
        let server = state.active.as_ref().cloned().ok_or(ErrorCode::Io)?;
        let generation = state.generation;
        let base_backing_fid = if fid == Fid::ROOT {
            Fid::ROOT
        } else {
            match state.fids.get(&fid) {
                Some(FidSlot::Bound {
                    generation: fid_gen,
                    backing_fid,
                }) if *fid_gen == generation => *backing_fid,
                Some(FidSlot::Bound { .. }) => return Err(ErrorCode::Io),
                Some(FidSlot::Reserved {
                    generation: fid_gen,
                    ..
                }) if *fid_gen == generation => return Err(ErrorCode::BadRequest),
                Some(FidSlot::Reserved { .. }) => return Err(ErrorCode::Io),
                None => return Err(ErrorCode::NotFound),
            }
        };
        if newfid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        if matches!(
            state.fids.get(&newfid),
            Some(
                FidSlot::Bound {
                    generation: existing_gen,
                    ..
                } | FidSlot::Reserved {
                    generation: existing_gen,
                    ..
                }
            ) if *existing_gen == generation
        ) {
            return Err(ErrorCode::BadRequest);
        }
        let backing_newfid = Fid(state.next_backing_fid);
        state.next_backing_fid = state.next_backing_fid.saturating_add(1);
        state.fids.insert(
            newfid,
            FidSlot::Reserved {
                generation,
                backing_fid: backing_newfid,
            },
        );
        drop(state);
        let guard = ReservationGuard {
            state: &self.state,
            client_fid: newfid,
            generation,
            backing_fid: backing_newfid,
            armed: true,
        };
        Ok(ReservedChildFid {
            server,
            base_backing_fid,
            backing_newfid,
            generation,
            guard,
        })
    }

    fn commit_child_fid(
        &self,
        newfid: Fid,
        generation: u64,
        backing_fid: Fid,
        guard: &mut ReservationGuard<'_>,
    ) -> Result<(), ErrorCode> {
        let mut state = self.lock_state();
        if state.active.is_some()
            && state.generation == generation
            && matches!(
                state.fids.get(&newfid),
                Some(FidSlot::Reserved {
                    generation: slot_gen,
                    backing_fid: slot_backing,
                }) if *slot_gen == generation && *slot_backing == backing_fid
            )
        {
            state.fids.insert(
                newfid,
                FidSlot::Bound {
                    generation,
                    backing_fid,
                },
            );
            guard.armed = false;
            Ok(())
        } else {
            Err(ErrorCode::Io)
        }
    }
}

#[async_trait]
impl FileServer for SupervisedHandle {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut reserved = self.reserve_child_fid(fid, newfid)?;
        let qid = reserved
            .server
            .walk(reserved.base_backing_fid, reserved.backing_newfid, names)
            .await?;
        self.commit_child_fid(
            newfid,
            reserved.generation,
            reserved.backing_newfid,
            &mut reserved.guard,
        )?;
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let (server, backing_fid, _) = self.resolve_active_fid(fid)?;
        server.open(backing_fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (server, backing_fid, _) = self.resolve_active_fid(fid)?;
        server.read(backing_fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let (server, backing_fid, _) = self.resolve_active_fid(fid)?;
        server.write(backing_fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let (server, backing_fid, _) = self.resolve_active_fid(fid)?;
        server.stat(backing_fid).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let mut reserved = self.reserve_child_fid(fid, newfid)?;
        let qid = reserved
            .server
            .create(
                reserved.base_backing_fid,
                reserved.backing_newfid,
                name,
                kind,
            )
            .await?;
        self.commit_child_fid(
            newfid,
            reserved.generation,
            reserved.backing_newfid,
            &mut reserved.guard,
        )?;
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let (server, backing_fid, generation) = self.resolve_active_fid(fid)?;
        server.remove(backing_fid).await?;
        if fid != Fid::ROOT {
            let mut state = self.lock_state();
            if matches!(
                state.fids.get(&fid),
                Some(FidSlot::Bound {
                    generation: slot_gen,
                    backing_fid: slot_backing,
                }) if *slot_gen == generation && *slot_backing == backing_fid
            ) {
                state.fids.remove(&fid);
            }
        }
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        let (server, backing_fid) = {
            let mut state = self.lock_state();
            if fid == Fid::ROOT {
                let server = state.active.as_ref().cloned().ok_or(ErrorCode::Io)?;
                (server, Fid::ROOT)
            } else if state.active.is_none() {
                state.fids.remove(&fid);
                return Err(ErrorCode::Io);
            } else {
                match state.fids.get(&fid).copied() {
                    None => return Err(ErrorCode::NotFound),
                    Some(FidSlot::Reserved { generation, .. })
                        if generation == state.generation =>
                    {
                        return Err(ErrorCode::BadRequest);
                    }
                    Some(FidSlot::Reserved { .. }) => {
                        state.fids.remove(&fid);
                        return Err(ErrorCode::Io);
                    }
                    Some(FidSlot::Bound { generation, .. }) if generation != state.generation => {
                        state.fids.remove(&fid);
                        return Err(ErrorCode::Io);
                    }
                    Some(FidSlot::Bound { backing_fid, .. }) => {
                        let server = state.active.as_ref().cloned().ok_or(ErrorCode::Io)?;
                        state.fids.remove(&fid);
                        (server, backing_fid)
                    }
                }
            }
        };
        server.clunk(backing_fid).await
    }
}
