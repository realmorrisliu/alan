//! Pure-aP client helpers for `/mnt/route` (`send`, `rules/*`, `ports/*`, `log`).

use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Request, Response, Stat};
use anyhow::{Context, Result, bail};

use super::{
    ROUTE_MOUNT_PATH, RouteHandoffMessage, RoutedRouteRecord, WorkstreamRouteRuleSpec,
    default_workstream_route_rules,
};

static HANDOFF_FID: AtomicU64 = AtomicU64::new(1 << 58);

fn next_fid() -> Fid {
    Fid(HANDOFF_FID.fetch_add(1, Ordering::Relaxed))
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_base(base_path: &str) -> &str {
    base_path.trim_end_matches('/')
}

async fn walk_path(fs: &InProcessTransport, path: &str) -> std::result::Result<Fid, ErrorCode> {
    let fid = next_fid();
    match fs
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_path(path),
        })
        .await?
    {
        Response::Walk { .. } => Ok(fid),
        _ => Err(ErrorCode::BadRequest),
    }
}

async fn clunk_quiet(fs: &InProcessTransport, fid: Fid) {
    let _ = fs.call(Request::Clunk { fid }).await;
}

async fn stat_path(fs: &InProcessTransport, path: &str) -> std::result::Result<Stat, ErrorCode> {
    let fid = walk_path(fs, path).await?;
    let stat = fs.call(Request::Stat { fid }).await;
    clunk_quiet(fs, fid).await;
    match stat? {
        Response::Stat { stat } => Ok(stat),
        _ => Err(ErrorCode::BadRequest),
    }
}

async fn write_all_chunks(
    fs: &InProcessTransport,
    fid: Fid,
    data: &[u8],
    path: &str,
) -> Result<()> {
    const CHUNK_SIZE: usize = 32 * 1024;
    let mut offset = 0usize;
    while offset < data.len() {
        let end = (offset + CHUNK_SIZE).min(data.len());
        match fs
            .call(Request::Write {
                fid,
                offset: offset as u64,
                data: data[offset..end].to_vec(),
            })
            .await
        {
            Ok(Response::Write { count }) => {
                if count == 0 {
                    bail!("zero-byte write to `{path}`");
                }
                offset += count as usize;
            }
            Ok(_) => bail!("unexpected write response for `{path}`"),
            Err(code) => bail!("write `{path}` failed: {code:?}"),
        }
    }
    Ok(())
}

/// Ensure default workstream handoff rules exist under `/mnt/route/rules/` via aP.
pub async fn ensure_workstream_route_rules(fs: &InProcessTransport) -> Result<()> {
    ensure_workstream_route_rules_at(fs, ROUTE_MOUNT_PATH).await
}

/// Ensure default workstream handoff rules exist under `<route_base_path>/rules/` via aP.
pub async fn ensure_workstream_route_rules_at(
    fs: &InProcessTransport,
    route_base_path: &str,
) -> Result<()> {
    let base = normalize_base(route_base_path);
    for (name, spec) in default_workstream_route_rules() {
        ensure_rule_file(fs, base, name, &spec).await?;
    }
    Ok(())
}

async fn ensure_rule_file(
    fs: &InProcessTransport,
    route_base: &str,
    name: &str,
    spec: &WorkstreamRouteRuleSpec,
) -> Result<()> {
    let full_path = format!("{route_base}/rules/{name}");
    if stat_path(fs, &full_path).await.is_ok() {
        return Ok(());
    }

    let rules_dir_path = format!("{route_base}/rules");
    let dir_fid = walk_path(fs, &rules_dir_path)
        .await
        .map_err(|code| anyhow::anyhow!("walk `{rules_dir_path}` failed: {code:?}"))?;
    let rule_fid = next_fid();

    let create_res = fs
        .call(Request::Create {
            fid: dir_fid,
            newfid: rule_fid,
            name: name.to_string(),
            kind: FileKind::File,
        })
        .await;
    clunk_quiet(fs, dir_fid).await;

    match create_res {
        Ok(Response::Create { .. }) => {}
        Ok(_) => bail!("unexpected create response for `{full_path}`"),
        Err(ErrorCode::BadRequest) if stat_path(fs, &full_path).await.is_ok() => {
            return Ok(());
        }
        Err(code) => bail!("create routefs rule `{full_path}` failed: {code:?}"),
    }

    let write_res = async {
        match fs
            .call(Request::Open {
                fid: rule_fid,
                mode: OpenMode::Write,
            })
            .await
        {
            Ok(Response::Open { .. }) => {}
            Ok(_) => bail!("unexpected open response for `{full_path}`"),
            Err(code) => bail!("open `{full_path}` failed: {code:?}"),
        }
        let bytes = serde_json::to_vec(spec)
            .with_context(|| format!("serialize rule spec for `{full_path}`"))?;
        write_all_chunks(fs, rule_fid, &bytes, &full_path).await?;
        match fs.call(Request::Clunk { fid: rule_fid }).await {
            Ok(Response::Clunk) => Ok(()),
            Ok(_) => bail!("unexpected clunk response for `{full_path}`"),
            Err(code) => bail!("commit `{full_path}` failed: {code:?}"),
        }
    }
    .await;

    if write_res.is_err() {
        clunk_quiet(fs, rule_fid).await;
    }
    write_res
}

/// Write a validated [`RouteHandoffMessage`] to `/mnt/route/send` and clunk to commit.
pub async fn send_handoff_message(
    fs: &InProcessTransport,
    message: &RouteHandoffMessage,
) -> Result<()> {
    send_handoff_message_at(fs, ROUTE_MOUNT_PATH, message).await
}

/// Write a validated [`RouteHandoffMessage`] to `<route_base_path>/send` and clunk to commit.
pub async fn send_handoff_message_at(
    fs: &InProcessTransport,
    route_base_path: &str,
    message: &RouteHandoffMessage,
) -> Result<()> {
    message.validate()?;
    let send_path = format!("{}/send", normalize_base(route_base_path));
    let bytes = serde_json::to_vec(message).context("serialize RouteHandoffMessage")?;

    let fid = walk_path(fs, &send_path)
        .await
        .map_err(|code| anyhow::anyhow!("walk `{send_path}` failed: {code:?}"))?;

    let send_res = async {
        match fs
            .call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await
        {
            Ok(Response::Open { .. }) => {}
            Ok(_) => bail!("unexpected open response for `{send_path}`"),
            Err(code) => bail!("open `{send_path}` failed: {code:?}"),
        }
        write_all_chunks(fs, fid, &bytes, &send_path).await?;
        match fs.call(Request::Clunk { fid }).await {
            Ok(Response::Clunk) => Ok(()),
            Ok(_) => bail!("unexpected clunk response for `{send_path}`"),
            Err(code) => bail!("commit `{send_path}` failed: {code:?}"),
        }
    }
    .await;

    if send_res.is_err() {
        clunk_quiet(fs, fid).await;
    }
    send_res
}

/// Incremental non-blocking NDJSON cursor for `/mnt/route/ports/*` and `/mnt/route/log`.
#[derive(Debug, Clone)]
pub struct RouteStreamCursor {
    path: String,
    offset: u64,
    line_buf: Vec<u8>,
}

impl RouteStreamCursor {
    /// Construct a cursor starting at offset 0 for `path`.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
            line_buf: Vec::new(),
        }
    }

    /// Return the current consumed byte offset in the stream.
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Non-blockingly drain any currently available complete NDJSON records from `self.path`.
    pub async fn poll_records(
        &mut self,
        fs: &InProcessTransport,
    ) -> Result<Vec<RoutedRouteRecord>> {
        let stat = stat_path(fs, &self.path)
            .await
            .map_err(|code| anyhow::anyhow!("stat `{}` failed: {code:?}", self.path))?;
        if stat.length <= self.offset {
            return Ok(Vec::new());
        }

        let fid = walk_path(fs, &self.path)
            .await
            .map_err(|code| anyhow::anyhow!("walk `{}` failed: {code:?}", self.path))?;
        let read_res = async {
            match fs
                .call(Request::Open {
                    fid,
                    mode: OpenMode::Read,
                })
                .await
            {
                Ok(Response::Open { .. }) => {}
                Ok(_) => bail!("unexpected open response for `{}`", self.path),
                Err(code) => bail!("open `{}` failed: {code:?}", self.path),
            }
            while self.offset < stat.length {
                let want = (stat.length - self.offset).min(32 * 1024) as u32;
                match fs
                    .call(Request::Read {
                        fid,
                        offset: self.offset,
                        count: want,
                    })
                    .await
                {
                    Ok(Response::Read { data }) if data.is_empty() => break,
                    Ok(Response::Read { data }) => {
                        self.offset += data.len() as u64;
                        self.line_buf.extend_from_slice(&data);
                    }
                    Ok(_) => bail!("unexpected read response for `{}`", self.path),
                    Err(code) => bail!("read `{}` failed: {code:?}", self.path),
                }
            }
            Ok(())
        }
        .await;
        clunk_quiet(fs, fid).await;
        read_res?;

        self.drain_complete_lines()
    }

    fn drain_complete_lines(&mut self) -> Result<Vec<RoutedRouteRecord>> {
        let mut records = Vec::new();
        while let Some(nl_pos) = self.line_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.line_buf.drain(..=nl_pos).collect();
            let text = std::str::from_utf8(&line)
                .with_context(|| format!("non-utf8 record in `{}`", self.path))?;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: RoutedRouteRecord = serde_json::from_str(trimmed)
                .with_context(|| format!("parse RoutedRouteRecord from `{}`", self.path))?;
            records.push(record);
        }
        Ok(records)
    }
}

/// Read all currently retained audit records from `/mnt/route/log`.
pub async fn read_route_audit_log(fs: &InProcessTransport) -> Result<Vec<RoutedRouteRecord>> {
    read_route_audit_log_at(fs, ROUTE_MOUNT_PATH).await
}

/// Read all currently retained audit records from `<route_base_path>/log`.
pub async fn read_route_audit_log_at(
    fs: &InProcessTransport,
    route_base_path: &str,
) -> Result<Vec<RoutedRouteRecord>> {
    let log_path = format!("{}/log", normalize_base(route_base_path));
    RouteStreamCursor::new(log_path).poll_records(fs).await
}
