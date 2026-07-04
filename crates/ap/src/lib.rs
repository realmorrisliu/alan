//! aP — Alan's file-service protocol (the 9P analog).
//!
//! Everything in Alan OS is a file; aP is the single contract every file server
//! and client speaks. This crate owns the protocol shape only: the
//! [`FileServer`] trait, fid/qid value types, byte/offset stream conventions,
//! the wire [`Request`]/[`Response`] messages, error codes, and the in-process
//! fast-path transport. It depends on no other Alan crate (ADR-0025 D1/D2).
//!
//! The contract is *wire-shaped* (ADR-0024 D5): every operation is expressible
//! over fids, paths, byte buffers, offsets, and error codes, so the same
//! operations can later cross a process boundary unchanged. v1 runs servers
//! in-process over [`InProcessTransport`] with no serialization cost.

pub mod reference;
mod server;
mod stream;
mod types;
mod version;
mod wire;

pub use server::{
    FileServer, ImportedFileServer, InProcessTransport, ProcessEvent, ProcessEventSink,
    ProcessEventSource, ProcessInputEventSink, ProcessInputEventSource, ProcessIoEventKind,
    ProcessIoEventSink, ProcessIoEventSource, ProcessOutputEventSink, ProcessOutputEventSource,
    WireTransportClient, export_file_server,
};
pub use stream::Stream;
pub use types::{ErrorCode, Fid, FileKind, Offset, OpenMode, Qid, Stat};
pub use version::VersionTable;
pub use wire::{
    MAX_WIRE_FRAME_BYTES, Request, Response, WireError, WireRequestFrame, WireResponseFrame,
    decode_request_frame, decode_response_frame, encode_request_frame, encode_response_frame,
    read_request_frame, read_response_frame, write_request_frame, write_response_frame,
};
