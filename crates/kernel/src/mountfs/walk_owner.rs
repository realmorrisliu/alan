//! A backing walk owns its fid before the outer namespace can publish it.
use super::{ErrorCode, Fid, Request, Resolved, Response, State};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) struct PendingWalk {
    state: Arc<Mutex<State>>,
    outer_fid: Fid,
    resolved: Resolved,
    fid: Option<Fid>,
}

async fn cleanup(state: Arc<Mutex<State>>, outer_fid: Fid, resolved: Resolved, fid: Fid) -> bool {
    let mut state = state.lock().await;
    let owned = state.fids.get(&outer_fid).is_some_and(|entry| {
        entry.reserved
            && entry
                .backing
                .as_ref()
                .is_some_and(|backing| backing.backing_fid == fid)
    });
    if owned {
        state.fids.remove(&outer_fid);
    }
    drop(state);
    let _ = resolved.call(Request::Clunk { fid }).await;
    owned
}

impl PendingWalk {
    pub(super) fn new(
        state: Arc<Mutex<State>>,
        outer_fid: Fid,
        resolved: Resolved,
        fid: Fid,
    ) -> Self {
        Self {
            state,
            outer_fid,
            resolved,
            fid: Some(fid),
        }
    }

    pub(super) fn transfer(&mut self) {
        self.fid = None;
    }

    pub(super) async fn wait(
        &self,
        cancelled: &mut tokio::sync::oneshot::Receiver<()>,
    ) -> Option<Result<Response, ErrorCode>> {
        tokio::select! {
            biased;
            _ = cancelled => None,
            result = self.resolved.call(Request::Walk {
                fid: Fid::ROOT, newfid: self.fid.expect("pending backing fid"),
                names: self.resolved.rel.clone(),
            }) => Some(result),
        }
    }

    pub(super) async fn close_backing(&self) {
        if let Some(fid) = self.fid {
            let _ =
                tokio::time::timeout(CLEANUP_TIMEOUT, self.resolved.call(Request::Clunk { fid }))
                    .await;
        }
    }

    pub(super) async fn close(&mut self) -> bool {
        let owned = if let Some(fid) = self.fid {
            tokio::time::timeout(
                CLEANUP_TIMEOUT,
                cleanup(
                    self.state.clone(),
                    self.outer_fid,
                    self.resolved.clone(),
                    fid,
                ),
            )
            .await
            .unwrap_or(false)
        } else {
            false
        };
        self.fid = None;
        owned
    }
}

impl Drop for PendingWalk {
    fn drop(&mut self) {
        if let Some(fid) = self.fid.take() {
            let state = self.state.clone();
            let resolved = self.resolved.clone();
            let outer_fid = self.outer_fid;
            tokio::spawn(async move {
                let _ =
                    tokio::time::timeout(CLEANUP_TIMEOUT, cleanup(state, outer_fid, resolved, fid))
                        .await;
            });
        }
    }
}
