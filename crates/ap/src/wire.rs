//! aP wire messages: the request/response pairs every operation reduces to.
//!
//! These are the bytes a transport carries (§5.7). The in-process fast path
//! ([`crate::InProcessTransport`]) dispatches the same `Request`/`Response`
//! values without serializing them; a future wire transport serializes them
//! unchanged. Keeping operations as explicit data — rather than method calls —
//! is what makes "a dumb byte transport could carry every operation" checkable.
//!
//! Fid allocation is client-driven (9P-faithful): `walk` and `create` carry the
//! `newfid` the client picked, `open` operates on an existing fid, and `clunk`
//! releases a fid (§5.2). Clone-via-open (§5.4) needs no dedicated operation:
//! the caller `open`s a [`FileKind::Clone`](crate::FileKind) file and `read`s it
//! to learn the allocated resource's name, then `walk`s to that name.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Stat};

/// Maximum newline-delimited aP wire frame accepted by the v1 byte transport.
pub const MAX_WIRE_FRAME_BYTES: usize = 1 << 20; // 1 MiB

/// One aP operation request. Inputs are fids, paths (name components), byte
/// buffers, offsets, and counts — nothing in-process-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Request {
    /// Walk `names` from `fid`, binding `newfid` to the destination file.
    Walk {
        fid: Fid,
        newfid: Fid,
        names: Vec<String>,
    },
    /// Open `fid` for the given access intent. On a clone file this allocates a
    /// new resource (§5.4); its name is then read from `fid`.
    Open { fid: Fid, mode: OpenMode },
    /// Read up to `count` bytes from `offset`. On a stream this blocks until new
    /// bytes are available at or after `offset` (§5.3).
    Read {
        fid: Fid,
        offset: Offset,
        count: u32,
    },
    /// Write `data` at `offset`. Document entry points commit on `clunk`, not on
    /// a partial write (commit-on-clunk).
    Write {
        fid: Fid,
        offset: Offset,
        data: Vec<u8>,
    },
    /// Return metadata for `fid`.
    Stat { fid: Fid },
    /// Create child `name` of `kind` under directory `fid`, binding `newfid` to it.
    Create {
        fid: Fid,
        newfid: Fid,
        name: String,
        kind: FileKind,
    },
    /// Remove the file `fid` refers to, then release `fid`.
    Remove { fid: Fid },
    /// Release `fid`. For commit-on-clunk document writes, this is the commit point.
    Clunk { fid: Fid },
}

/// One aP operation response. Every output is owned and serializable (no borrows
/// into server memory), so it survives a byte transport unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Response {
    Walk { qid: Qid },
    Open { qid: Qid },
    Read { data: Vec<u8> },
    Write { count: u32 },
    Stat { stat: Stat },
    Create { qid: Qid },
    Remove,
    Clunk,
}

/// A newline-delimited request frame carried by the aP byte transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireRequestFrame {
    pub request: Request,
}

/// A newline-delimited response frame carried by the aP byte transport.
///
/// Successful aP operation results and typed [`ErrorCode`] failures are both
/// first-class protocol results. Transport IO/codec failures stay outside this
/// envelope and map to [`WireError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum WireResponseFrame {
    Ok { response: Response },
    Error { code: ErrorCode },
}

impl WireResponseFrame {
    pub fn from_result(result: &Result<Response, ErrorCode>) -> Self {
        match result {
            Ok(response) => Self::Ok {
                response: response.clone(),
            },
            Err(code) => Self::Error { code: *code },
        }
    }

    pub fn into_result(self) -> Result<Response, ErrorCode> {
        match self {
            Self::Ok { response } => Ok(response),
            Self::Error { code } => Err(code),
        }
    }
}

/// Errors in the byte transport itself, separate from aP operation failures.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("aP wire io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("aP wire codec error: {0}")]
    Codec(#[from] serde_json::Error),
    #[error("aP wire peer closed before a response frame")]
    Closed,
    #[error("aP wire connection has an abandoned in-flight request")]
    Unsynchronized,
    #[error("aP wire frame exceeds {max} bytes")]
    FrameTooLarge { max: usize },
}

impl WireError {
    /// Map transport-level failures back into typed aP operation errors for
    /// imported file-server adapters.
    pub fn to_error_code(&self) -> ErrorCode {
        match self {
            Self::Codec(_) | Self::FrameTooLarge { .. } => ErrorCode::BadRequest,
            Self::Io(_) | Self::Closed | Self::Unsynchronized => ErrorCode::Io,
        }
    }
}

pub fn encode_request_frame(request: &Request) -> Result<Vec<u8>, WireError> {
    encode_json_line(&WireRequestFrame {
        request: request.clone(),
    })
}

pub fn decode_request_frame(frame: &[u8]) -> Result<Request, WireError> {
    let frame: WireRequestFrame = serde_json::from_slice(frame)?;
    Ok(frame.request)
}

pub fn encode_response_frame(result: &Result<Response, ErrorCode>) -> Result<Vec<u8>, WireError> {
    encode_json_line(&WireResponseFrame::from_result(result))
}

pub fn decode_response_frame(frame: &[u8]) -> Result<Result<Response, ErrorCode>, WireError> {
    let frame: WireResponseFrame = serde_json::from_slice(frame)?;
    Ok(frame.into_result())
}

pub async fn write_request_frame<W>(writer: &mut W, request: &Request) -> Result<(), WireError>
where
    W: AsyncWrite + Unpin,
{
    let frame = encode_request_frame(request)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_request_frame<R>(reader: &mut R) -> Result<Option<Request>, WireError>
where
    R: AsyncBufRead + Unpin,
{
    let Some(frame) = read_json_line(reader).await? else {
        return Ok(None);
    };
    decode_request_frame(&frame).map(Some)
}

pub async fn write_response_frame<W>(
    writer: &mut W,
    result: &Result<Response, ErrorCode>,
) -> Result<(), WireError>
where
    W: AsyncWrite + Unpin,
{
    let frame = encode_response_frame(result)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_response_frame<R>(
    reader: &mut R,
) -> Result<Option<Result<Response, ErrorCode>>, WireError>
where
    R: AsyncBufRead + Unpin,
{
    let Some(frame) = read_json_line(reader).await? else {
        return Ok(None);
    };
    decode_response_frame(&frame).map(Some)
}

fn encode_json_line<T>(value: &T) -> Result<Vec<u8>, WireError>
where
    T: Serialize,
{
    let mut frame = serde_json::to_vec(value)?;
    frame.push(b'\n');
    Ok(frame)
}

async fn read_json_line<R>(reader: &mut R) -> Result<Option<Vec<u8>>, WireError>
where
    R: AsyncBufRead + Unpin,
{
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Ok(Some(frame))
            };
        }

        let consumed = if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
            newline + 1
        } else {
            available.len()
        };
        if frame.len().saturating_add(consumed) > MAX_WIRE_FRAME_BYTES {
            return Err(WireError::FrameTooLarge {
                max: MAX_WIRE_FRAME_BYTES,
            });
        }
        frame.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);

        if frame.last() == Some(&b'\n') {
            return Ok(Some(frame));
        }
    }
}
