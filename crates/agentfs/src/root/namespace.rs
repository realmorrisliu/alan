//! Namespace mechanics for the `/agent` overlay.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, atomic::Ordering},
};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid};

use super::{NEXT_BACKING_FID, Node};

pub(super) async fn rollback_created_fid(server: &Arc<dyn FileServer>, fid: Fid) {
    if server.remove(fid).await.is_err() {
        let _ = server.clunk(fid).await;
    }
}

pub(super) async fn read_file_text(
    server: Arc<dyn FileServer>,
    names: &[String],
    raw_fid: u64,
) -> Result<String, ErrorCode> {
    let fid = Fid(raw_fid);
    server.walk(Fid::ROOT, fid, names).await?;
    let result = match server.open(fid, OpenMode::Read).await {
        Ok(_) => {
            let length = match server.stat(fid).await {
                Ok(stat) => stat.length,
                Err(e) => {
                    let _ = server.clunk(fid).await;
                    return Err(e);
                }
            };
            match server
                .read(
                    fid,
                    0,
                    u32::try_from(length.saturating_add(1)).unwrap_or(u32::MAX),
                )
                .await
            {
                Ok(bytes) => String::from_utf8(bytes).map_err(|_| ErrorCode::Io),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    };
    let clunk = server.clunk(fid).await;
    let text = result?;
    clunk?;
    Ok(text)
}

pub(super) async fn read_listing(
    server: Arc<dyn FileServer>,
    names: &[String],
) -> Result<Vec<String>, ErrorCode> {
    let fid = Fid(NEXT_BACKING_FID.fetch_add(1, Ordering::Relaxed));
    server.walk(Fid::ROOT, fid, names).await?;
    server.open(fid, OpenMode::Read).await?;
    let length = server.stat(fid).await?.length;
    let bytes = server
        .read(
            fid,
            0,
            u32::try_from(length.saturating_add(1)).unwrap_or(u32::MAX),
        )
        .await;
    let clunk = server.clunk(fid).await;
    let bytes = bytes?;
    clunk?;
    let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
    Ok(text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub(super) fn is_proc_overlay_name(name: &str) -> bool {
    matches!(
        name,
        "status" | "parent" | "credentials" | "exit" | "ctl" | "namespace" | "io"
    )
}

pub(super) fn proc_child_names(mut names: Vec<String>, child: &str) -> Vec<String> {
    names.push(child.to_string());
    names
}

pub(super) fn is_agent_overlay_reserved_name(name: &str) -> bool {
    matches!(name, "children" | "io") || is_proc_overlay_name(name)
}

pub(super) fn agent_children_qid(pid: &str, children: &[String]) -> Qid {
    Qid {
        kind: FileKind::Dir,
        version: hash_value(&("agent-children-version", pid, children)) as u32,
        path: hash_value(&("agent-children", pid)),
    }
}

pub(super) fn root_qid(listing: &[String]) -> Qid {
    Qid {
        kind: FileKind::Dir,
        version: hash_value(&("agent-root-version", listing)) as u32,
        path: 0xA6E7,
    }
}

pub(super) async fn release_node(node: Node) {
    match node {
        Node::AgentFile {
            backing,
            backing_fid,
            ..
        } => {
            let _ = backing.clunk(backing_fid).await;
        }
        Node::ProcFile { proc, proc_fid, .. } => {
            let _ = proc.clunk(proc_fid).await;
        }
        Node::Root | Node::AgentRoot { .. } | Node::AgentChildren { .. } => {}
    }
}

pub(super) fn namespace_agent_qid(pid: &str, qid: Qid) -> Qid {
    Qid {
        kind: qid.kind,
        version: qid.version,
        path: hash_value(&("agent-qid", pid, qid.path, file_kind_tag(qid.kind))),
    }
}

fn file_kind_tag(kind: FileKind) -> u8 {
    match kind {
        FileKind::Dir => 1,
        FileKind::File => 2,
        FileKind::Stream => 3,
        FileKind::Clone => 4,
    }
}

fn hash_value<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
