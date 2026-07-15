use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Request, Response, Stat};
use anyhow::{Context, Result, bail};

static NEXT_FID: AtomicU64 = AtomicU64::new(10_000);

pub(super) struct InputFrame {
    pub(super) message: String,
    pub(super) bytes_consumed: usize,
}

impl InputFrame {
    fn total_len(raw: &[u8]) -> Result<Option<usize>> {
        let Some(nl) = raw.iter().position(|&b| b == b'\n') else {
            return Ok(None);
        };
        let len: usize = std::str::from_utf8(&raw[..nl])
            .context("input frame length is not utf8")?
            .parse()
            .context("input frame length is not a number")?;
        let start = nl + 1;
        let end = start
            .checked_add(len)
            .context("input frame length overflowed")?;
        Ok(Some(end))
    }

    pub(super) fn parse_one(raw: &[u8]) -> Result<Self> {
        let end = Self::total_len(raw)?.context("input frame is missing length header")?;
        if raw.len() < end {
            bail!("input frame is truncated");
        }
        let start = raw
            .iter()
            .position(|&b| b == b'\n')
            .expect("total_len requires a length header")
            + 1;
        let message = String::from_utf8(raw[start..end].to_vec())
            .context("input frame payload is not utf8")?;
        Ok(Self {
            message,
            bytes_consumed: end,
        })
    }
}

#[derive(Clone)]
pub(super) struct NamespaceClient {
    fs: InProcessTransport,
}

pub(super) struct NamespaceFidGuard {
    client: NamespaceClient,
    fid: Option<Fid>,
}

impl NamespaceFidGuard {
    fn new(client: NamespaceClient, fid: Fid) -> Self {
        Self {
            client,
            fid: Some(fid),
        }
    }

    pub(super) fn fid(&self) -> Fid {
        self.fid.expect("namespace fid guard is closed")
    }

    pub(super) async fn close(mut self) -> Result<()> {
        let Some(fid) = self.fid.take() else {
            return Ok(());
        };
        self.client.clunk(fid).await
    }
}

impl Drop for NamespaceFidGuard {
    fn drop(&mut self) {
        let Some(fid) = self.fid.take() else {
            return;
        };
        let client = self.client.clone();
        drop(tokio::spawn(async move {
            let _ = client.clunk(fid).await;
        }));
    }
}

impl NamespaceClient {
    pub(super) fn new(fs: InProcessTransport) -> Self {
        Self { fs }
    }

    pub(super) async fn walk_to(&self, path: &str) -> Result<Fid> {
        let fid = Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await?
        {
            Response::Walk { .. } => Ok(fid),
            _ => bail!("unexpected walk response for {path}"),
        }
    }

    pub(super) async fn open(&self, fid: Fid, mode: OpenMode) -> Result<FileKind> {
        match self.fs.call(Request::Open { fid, mode }).await? {
            Response::Open { qid } => Ok(qid.kind),
            _ => bail!("unexpected open response"),
        }
    }

    async fn open_guarded_fid(&self, fid: Fid, mode: OpenMode) -> Result<NamespaceFidGuard> {
        match self.open(fid, mode).await {
            Ok(_) => Ok(NamespaceFidGuard::new(self.clone(), fid)),
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    pub(super) async fn open_path_guarded(
        &self,
        path: &str,
        mode: OpenMode,
    ) -> Result<NamespaceFidGuard> {
        let fid = self.walk_to(path).await?;
        self.open_guarded_fid(fid, mode).await
    }

    pub(super) async fn read_at(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>> {
        match self.fs.call(Request::Read { fid, offset, count }).await? {
            Response::Read { data } => Ok(data),
            _ => bail!("unexpected read response"),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat> {
        match self.fs.call(Request::Stat { fid }).await? {
            Response::Stat { stat } => Ok(stat),
            _ => bail!("unexpected stat response"),
        }
    }

    async fn read_all_opened(&self, fid: Fid) -> Result<Vec<u8>> {
        let stat = self.stat(fid).await?;
        let bounded_length =
            matches!(stat.qid.kind, FileKind::File | FileKind::Stream).then_some(stat.length);
        let mut offset = 0_u64;
        let mut data = Vec::new();
        loop {
            let count = bounded_length
                .map(|length| length.saturating_sub(offset).min(64 * 1024) as u32)
                .unwrap_or(64 * 1024);
            if count == 0 {
                break;
            }
            let chunk = self.read_at(fid, offset, count).await?;
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    pub(super) async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let fid = self.open_path_guarded(path, OpenMode::Read).await?;
        let data = self.read_all_opened(fid.fid()).await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    pub(super) async fn try_read_directory_names(&self, path: &str) -> Result<Option<Vec<String>>> {
        let fid = Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await
        {
            Ok(Response::Walk { .. }) => {}
            Ok(_) => bail!("unexpected walk response for {path}"),
            Err(ErrorCode::NotFound) => return Ok(None),
            Err(error) => return Err(error).with_context(|| format!("walk to {path}")),
        }
        let guarded = self.open_guarded_fid(fid, OpenMode::Read).await?;
        if self.stat(guarded.fid()).await?.qid.kind != FileKind::Dir {
            bail!("{path} is not a directory");
        }
        let mut bytes = Vec::new();
        let mut offset = 0_u64;
        loop {
            let chunk = self.read_at(guarded.fid(), offset, 64 * 1024).await?;
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            bytes.extend_from_slice(&chunk);
        }
        guarded.close().await?;
        let listing = String::from_utf8(bytes).with_context(|| format!("read directory {path}"))?;
        Ok(Some(listing.lines().map(str::to_string).collect()))
    }

    pub(super) async fn try_read_file(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let fid = Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await
        {
            Ok(Response::Walk { .. }) => {}
            Ok(_) => bail!("unexpected walk response for {path}"),
            Err(ErrorCode::NotFound) => return Ok(None),
            Err(err) => return Err(err).with_context(|| format!("walk to {path}")),
        }

        let fid = self.open_guarded_fid(fid, OpenMode::Read).await?;
        let data = self.read_all_opened(fid.fid()).await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(Some(data)),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    pub(super) async fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>> {
        let fid = self.open_path_guarded(path, OpenMode::Read).await?;
        let data = self.read_range_opened(fid.fid(), offset, length).await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    pub(super) async fn stat_path(&self, path: &str) -> Result<Stat> {
        let fid = self.walk_to(path).await?;
        let stat = self.stat(fid).await;
        let clunk = self.clunk(fid).await;
        match (stat, clunk) {
            (Ok(stat), Ok(())) => Ok(stat),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    pub(super) async fn write_document(&self, path: &str, data: &[u8]) -> Result<()> {
        let fid = self.walk_to(path).await?;
        match self.write_opened(fid, data).await {
            Ok(()) => self.clunk(fid).await,
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
        .with_context(|| format!("write {path}"))
    }

    async fn write_opened(&self, fid: Fid, data: &[u8]) -> Result<()> {
        self.open(fid, OpenMode::Write).await?;
        self.write_all_opened(fid, data).await
    }

    async fn write_all_opened(&self, fid: Fid, data: &[u8]) -> Result<()> {
        let mut offset = 0_u64;
        let mut remaining = data;
        if remaining.is_empty() {
            self.write_at(fid, 0, remaining).await?;
            return Ok(());
        }
        while !remaining.is_empty() {
            let written = self.write_at(fid, offset, remaining).await?;
            if written == 0 || written > remaining.len() {
                bail!("invalid write count from file server");
            }
            offset += written as u64;
            remaining = &remaining[written..];
        }
        Ok(())
    }

    pub(super) async fn write_at(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<usize> {
        match self
            .fs
            .call(Request::Write {
                fid,
                offset,
                data: data.to_vec(),
            })
            .await?
        {
            Response::Write { count } => Ok(count as usize),
            _ => bail!("unexpected write response"),
        }
    }

    pub(super) async fn clone_via_open(&self, path: &str) -> Result<String> {
        let fid = self.walk_to(path).await?;
        match async {
            self.open(fid, OpenMode::ReadWrite).await?;
            let id = String::from_utf8(self.read_at(fid, 0, 128).await?)
                .with_context(|| format!("{path} returned non-utf8 id"))?;
            Ok(id)
        }
        .await
        {
            Ok(id) => {
                self.clunk(fid).await?;
                Ok(id)
            }
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    pub(super) async fn clone_with_document(&self, path: &str, data: &[u8]) -> Result<String> {
        let fid = self.walk_to(path).await?;
        match async {
            self.open(fid, OpenMode::ReadWrite).await?;
            let id = String::from_utf8(self.read_at(fid, 0, 128).await?)
                .with_context(|| format!("{path} returned non-utf8 id"))?;
            self.write_all_opened(fid, data).await?;
            Ok(id)
        }
        .await
        {
            Ok(id) => {
                self.clunk(fid).await?;
                Ok(id)
            }
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    pub(super) async fn read_stream_from(&self, path: &str, offset: u64) -> Result<Vec<u8>> {
        let fid = self.open_path_guarded(path, OpenMode::Read).await?;
        let data = async {
            let mut data = self.read_at(fid.fid(), offset, 64 * 1024).await?;
            let total_len =
                InputFrame::total_len(&data)?.context("input frame is missing length header")?;
            while data.len() < total_len {
                let remaining = total_len - data.len();
                let count = remaining.min(64 * 1024) as u32;
                if count == 0 {
                    bail!("input frame is truncated");
                }
                let chunk = self
                    .read_at(fid.fid(), offset + data.len() as u64, count)
                    .await?;
                if chunk.is_empty() {
                    bail!("input frame is truncated");
                }
                data.extend_from_slice(&chunk);
            }
            Ok(data)
        }
        .await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    async fn read_range_opened(&self, fid: Fid, offset: u64, length: u64) -> Result<Vec<u8>> {
        let mut next_offset = offset;
        let mut remaining = length;
        let mut data = Vec::with_capacity(length.min(64 * 1024) as usize);
        while remaining > 0 {
            let count = remaining.min(64 * 1024) as u32;
            let chunk = self.read_at(fid, next_offset, count).await?;
            if chunk.is_empty() {
                bail!("file ended before requested range was reached");
            }
            next_offset += chunk.len() as u64;
            remaining = remaining
                .checked_sub(chunk.len() as u64)
                .context("read exceeded requested range")?;
            data.extend_from_slice(&chunk);
        }
        Ok(data)
    }

    pub(super) async fn clunk(&self, fid: Fid) -> Result<()> {
        match self.fs.call(Request::Clunk { fid }).await? {
            Response::Clunk => Ok(()),
            _ => bail!("unexpected clunk response"),
        }
    }
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
