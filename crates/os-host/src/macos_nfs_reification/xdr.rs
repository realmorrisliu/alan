//! Bounds-checked XDR (RFC 4506) binary reader and writer for ONC RPC / NFSv3.
//!
//! Enforces 4-byte alignment padding on opaque and string payloads and caps
//! records at [`MAX_XDR_RECORD_BYTES`] (2 MiB) so malformed or hostile frames
//! cannot trigger unbounded allocations or out-of-bounds slicing.

use std::fmt;

/// Hard upper bound (2 MiB) on any single XDR record read or written.
pub const MAX_XDR_RECORD_BYTES: usize = 2 * 1024 * 1024;

/// Errors returned while decoding or encoding XDR payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XdrError {
    /// Input buffer ended before the requested field or alignment padding completed.
    UnexpectedEof,
    /// Record or field exceeds the configured maximum byte cap.
    RecordTooLarge { len: usize, max: usize },
    /// Arithmetic overflow while computing padded XDR lengths.
    LengthOverflow,
    /// XDR boolean wire value was neither `0` nor `1`.
    InvalidBool(u32),
    /// XDR string payload was not valid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for XdrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of XDR buffer"),
            Self::RecordTooLarge { len, max } => {
                write!(f, "XDR record size {len} exceeds maximum {max} bytes")
            }
            Self::LengthOverflow => write!(f, "XDR length arithmetic overflow"),
            Self::InvalidBool(v) => write!(f, "invalid XDR boolean value {v}"),
            Self::InvalidUtf8 => write!(f, "invalid UTF-8 in XDR string"),
        }
    }
}

impl std::error::Error for XdrError {}

/// Compute the 4-byte aligned XDR byte length for a payload of `len` bytes.
pub const fn xdr_padded_len(len: usize) -> Option<usize> {
    let rem = len % 4;
    let pad = if rem == 0 { 0 } else { 4 - rem };
    len.checked_add(pad)
}

/// Bounds-checked big-endian XDR reader over a borrowed byte slice.
#[derive(Debug, Clone)]
pub struct XdrReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> XdrReader<'a> {
    /// Construct a reader enforcing the default [`MAX_XDR_RECORD_BYTES`] (2 MiB) cap.
    pub fn new(buf: &'a [u8]) -> Result<Self, XdrError> {
        Self::with_limit(buf, MAX_XDR_RECORD_BYTES)
    }

    /// Construct a reader enforcing an explicit `max_len` cap.
    pub fn with_limit(buf: &'a [u8], max_len: usize) -> Result<Self, XdrError> {
        if buf.len() > max_len {
            return Err(XdrError::RecordTooLarge {
                len: buf.len(),
                max: max_len,
            });
        }
        Ok(Self { buf, pos: 0 })
    }

    /// Number of unread bytes remaining in the buffer.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Current cursor byte offset.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Read a 4-byte big-endian unsigned integer.
    pub fn read_u32(&mut self) -> Result<u32, XdrError> {
        let bytes = self.take_exact(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a 4-byte big-endian signed integer.
    pub fn read_i32(&mut self) -> Result<i32, XdrError> {
        let bytes = self.take_exact(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read an 8-byte big-endian unsigned hyper integer.
    pub fn read_u64(&mut self) -> Result<u64, XdrError> {
        let bytes = self.take_exact(8)?;
        Ok(u64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a 4-byte XDR boolean (`0` = false, `1` = true).
    pub fn read_bool(&mut self) -> Result<bool, XdrError> {
        match self.read_u32()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(XdrError::InvalidBool(other)),
        }
    }

    /// Read a fixed-length opaque byte slice plus its 4-byte alignment padding.
    pub fn read_fixed_opaque(&mut self, len: usize) -> Result<&'a [u8], XdrError> {
        let padded = xdr_padded_len(len).ok_or(XdrError::LengthOverflow)?;
        let slice = self.take_exact(padded)?;
        Ok(&slice[..len])
    }

    /// Read a length-prefixed variable opaque byte slice up to [`MAX_XDR_RECORD_BYTES`].
    pub fn read_opaque(&mut self) -> Result<&'a [u8], XdrError> {
        self.read_opaque_bounded(MAX_XDR_RECORD_BYTES)
    }

    /// Read a length-prefixed variable opaque byte slice bounded by `max_len`.
    pub fn read_opaque_bounded(&mut self, max_len: usize) -> Result<&'a [u8], XdrError> {
        let len = usize::try_from(self.read_u32()?).map_err(|_| XdrError::LengthOverflow)?;
        if len > max_len {
            return Err(XdrError::RecordTooLarge { len, max: max_len });
        }
        self.read_fixed_opaque(len)
    }

    /// Read a length-prefixed UTF-8 XDR string up to [`MAX_XDR_RECORD_BYTES`].
    pub fn read_string(&mut self) -> Result<String, XdrError> {
        self.read_string_bounded(MAX_XDR_RECORD_BYTES)
    }

    /// Read a length-prefixed UTF-8 XDR string bounded by `max_len`.
    pub fn read_string_bounded(&mut self, max_len: usize) -> Result<String, XdrError> {
        let bytes = self.read_opaque_bounded(max_len)?;
        let text = std::str::from_utf8(bytes).map_err(|_| XdrError::InvalidUtf8)?;
        Ok(text.to_string())
    }

    fn take_exact(&mut self, len: usize) -> Result<&'a [u8], XdrError> {
        let end = self.pos.checked_add(len).ok_or(XdrError::LengthOverflow)?;
        if end > self.buf.len() {
            return Err(XdrError::UnexpectedEof);
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
}

/// Bounds-checked big-endian XDR writer enforcing a maximum record length.
#[derive(Debug, Clone)]
pub struct XdrWriter {
    buf: Vec<u8>,
    max_len: usize,
}

impl Default for XdrWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl XdrWriter {
    /// Construct an empty writer capped at [`MAX_XDR_RECORD_BYTES`] (2 MiB).
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            max_len: MAX_XDR_RECORD_BYTES,
        }
    }

    /// Construct a writer with initial capacity capped at [`MAX_XDR_RECORD_BYTES`].
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity.min(MAX_XDR_RECORD_BYTES)),
            max_len: MAX_XDR_RECORD_BYTES,
        }
    }

    /// Construct a writer with an explicit maximum byte cap.
    pub fn with_limit(max_len: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_len,
        }
    }

    /// Current encoded length in bytes.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether no bytes have been written yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Borrow the encoded bytes written so far.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    /// Consume the writer and return the encoded byte buffer.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// Write a 4-byte big-endian unsigned integer.
    pub fn write_u32(&mut self, value: u32) -> Result<(), XdrError> {
        self.reserve_within_limit(4)?;
        self.buf.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    /// Write a 4-byte big-endian signed integer.
    pub fn write_i32(&mut self, value: i32) -> Result<(), XdrError> {
        self.reserve_within_limit(4)?;
        self.buf.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    /// Write an 8-byte big-endian unsigned hyper integer.
    pub fn write_u64(&mut self, value: u64) -> Result<(), XdrError> {
        self.reserve_within_limit(8)?;
        self.buf.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    /// Write a 4-byte XDR boolean.
    pub fn write_bool(&mut self, value: bool) -> Result<(), XdrError> {
        self.write_u32(u32::from(value))
    }

    /// Write a fixed-length opaque byte slice followed by zero bytes to a 4-byte boundary.
    pub fn write_fixed_opaque(&mut self, bytes: &[u8]) -> Result<(), XdrError> {
        let padded = xdr_padded_len(bytes.len()).ok_or(XdrError::LengthOverflow)?;
        let pad = padded - bytes.len();
        self.reserve_within_limit(padded)?;
        self.buf.extend_from_slice(bytes);
        if pad > 0 {
            self.buf.extend_from_slice(&[0u8; 3][..pad]);
        }
        Ok(())
    }

    /// Write a length-prefixed variable opaque byte slice with 4-byte zero padding.
    pub fn write_opaque(&mut self, bytes: &[u8]) -> Result<(), XdrError> {
        let len_u32 = u32::try_from(bytes.len()).map_err(|_| XdrError::LengthOverflow)?;
        let padded = xdr_padded_len(bytes.len()).ok_or(XdrError::LengthOverflow)?;
        let total = 4usize.checked_add(padded).ok_or(XdrError::LengthOverflow)?;
        self.reserve_within_limit(total)?;
        self.buf.extend_from_slice(&len_u32.to_be_bytes());
        self.buf.extend_from_slice(bytes);
        let pad = padded - bytes.len();
        if pad > 0 {
            self.buf.extend_from_slice(&[0u8; 3][..pad]);
        }
        Ok(())
    }

    /// Write a length-prefixed UTF-8 string with 4-byte zero padding.
    pub fn write_string(&mut self, value: &str) -> Result<(), XdrError> {
        self.write_opaque(value.as_bytes())
    }

    /// Append raw already-encoded XDR bytes while enforcing the record byte cap.
    pub fn write_raw_bytes(&mut self, bytes: &[u8]) -> Result<(), XdrError> {
        self.reserve_within_limit(bytes.len())?;
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    fn reserve_within_limit(&self, additional: usize) -> Result<(), XdrError> {
        let next_len = self
            .buf
            .len()
            .checked_add(additional)
            .ok_or(XdrError::LengthOverflow)?;
        if next_len > self.max_len {
            return Err(XdrError::RecordTooLarge {
                len: next_len,
                max: self.max_len,
            });
        }
        Ok(())
    }
}
