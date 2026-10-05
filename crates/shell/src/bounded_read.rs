//! Finite reads retain their descriptor owner across acquisition and cleanup.
use super::*;
use std::time::Duration;

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

struct ReadFid {
    shell: Shell,
    fid: Option<Fid>,
}

impl ReadFid {
    async fn close(&mut self) -> Result<(), ErrorCode> {
        let result = tokio::time::timeout(
            CLEANUP_TIMEOUT,
            self.shell.clunk(self.fid.expect("read fid owned")),
        )
        .await
        .map_err(|_| ErrorCode::Io)
        .and_then(|result| result);
        self.fid = None;
        result
    }
}

impl Drop for ReadFid {
    fn drop(&mut self) {
        // An aborted caller still owes cleanup for a walk that already bound
        // its fid. The cleanup itself must not retain a stuck server forever.
        if let Some(fid) = self.fid.take() {
            let shell = self.shell.clone();
            tokio::spawn(async move {
                let _ = tokio::time::timeout(CLEANUP_TIMEOUT, shell.clunk(fid)).await;
            });
        }
    }
}

impl Shell {
    /// Read an exact finite range, or the current remainder when `length` is
    /// `None`. One timeout covers walk, open, stat and all reads; cleanup has a
    /// separate one-second bound and is also attempted if the caller is aborted.
    /// Oversized, missing, short and timed-out ranges return an error.
    pub async fn read_range_bounded(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
        max_bytes: u64,
        timeout: Duration,
    ) -> Result<Vec<u8>, ErrorCode> {
        let fid = self.alloc_fid();
        let mut owner = ReadFid {
            shell: self.clone(),
            fid: Some(fid),
        };
        let result = tokio::time::timeout(timeout, async {
            match self
                .fs
                .call(Request::Walk {
                    fid: Fid::ROOT,
                    newfid: fid,
                    names: split_path(path),
                })
                .await?
            {
                Response::Walk { .. } => {}
                _ => return Err(ErrorCode::Io),
            }
            self.open(fid, OpenMode::Read).await?;
            let available = self.length(fid).await?;
            let remaining = available.checked_sub(offset).ok_or(ErrorCode::NotFound)?;
            let length = length.unwrap_or(remaining);
            offset.checked_add(length).ok_or(ErrorCode::BadRequest)?;
            if length > max_bytes {
                return Err(ErrorCode::BadRequest);
            }
            if length > remaining {
                return Err(ErrorCode::NotFound);
            }
            let mut bytes = Vec::new();
            while (bytes.len() as u64) < length {
                let count = (length - bytes.len() as u64).min(4096) as u32;
                let chunk = self
                    .read_at(fid, offset + bytes.len() as u64, count)
                    .await?;
                if chunk.is_empty() || chunk.len() > count as usize {
                    return Err(ErrorCode::Io);
                }
                bytes.extend(chunk);
            }
            Ok(bytes)
        })
        .await
        .map_err(|_| ErrorCode::Io)
        .and_then(|result| result);
        let close = owner.close().await;
        match result {
            Err(error) => Err(error),
            Ok(bytes) => close.map(|()| bytes),
        }
    }
}
