//! Namespace tree nodes, identities, access predicates, and read projections for `LlmFs`.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use alan_ap::{ErrorCode, FileKind, Offset};

use crate::State;
use crate::generation::generation_status_doc;
use crate::provider_catalog::{
    connection_capabilities_doc, connection_models_doc, connection_profile_doc,
    known_provider_names, provider_capabilities_doc, provider_capabilities_for_name,
    provider_models_doc, provider_status_doc,
};

/// What a fid points at within the llmfs tree.
#[derive(Clone)]
pub(crate) enum Node {
    Root,
    ProvidersDir,
    Provider(String),
    ProviderModels(String),
    ProviderCapabilities(String),
    ProviderStatus(String),
    ConnectionsDir,
    Connection(String),
    ConnectionProvider(String),
    ConnectionProfile(String),
    ConnectionMeter(String),
    ConnectionCapabilities(String),
    ConnectionModels(String),
    Clone(String),
    Evaluate(String),
    Decide(String),
    Gen(String),
    GenData(String),
    GenEvents(String),
    GenCtl(String),
    GenStatus(String),
}

/// Render a readable node's bytes from already-locked state (so both `read` and
/// `stat`'s length use one definition).
pub(crate) fn computed_bytes(
    state: &State,
    node: &Node,
    allowed_connections: Option<&HashSet<String>>,
) -> Result<Vec<u8>, ErrorCode> {
    let bytes = match node {
        Node::Root => b"connections\nproviders".to_vec(),
        Node::ProvidersDir => known_provider_names().join("\n").into_bytes(),
        Node::Provider(_) => b"models\ncapabilities\nstatus".to_vec(),
        Node::ProviderModels(provider) => provider_models_doc(provider).into_bytes(),
        Node::ProviderCapabilities(provider) => {
            provider_capabilities_doc(provider, provider_capabilities_for_name(provider))
                .into_bytes()
        }
        Node::ProviderStatus(provider) => provider_status_doc(provider).into_bytes(),
        Node::ConnectionsDir => {
            let mut names: Vec<_> = state
                .connections
                .keys()
                .filter(|name| allowed_connections.is_none_or(|allowed| allowed.contains(*name)))
                .cloned()
                .collect();
            names.sort();
            names.join("\n").into_bytes()
        }
        // A connection lists `clone`, `evaluate`, `decide` plus its allocated
        // operation ids, so a permitted observer can discover live/finished
        // operations as files.
        Node::Connection(conn) => {
            let mut names = vec![
                "clone".to_string(),
                "evaluate".to_string(),
                "decide".to_string(),
                "provider".to_string(),
                "profile".to_string(),
                "meter".to_string(),
                "capabilities".to_string(),
                "models".to_string(),
            ];
            let mut ids: Vec<_> = state
                .gens
                .iter()
                .filter(|(_, g)| &g.connection_name() == conn)
                .map(|(id, _)| id.clone())
                .collect();
            ids.sort();
            names.extend(ids);
            names.join("\n").into_bytes()
        }
        Node::ConnectionProvider(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            format!("{}\n", connection.provider_name).into_bytes()
        }
        Node::ConnectionProfile(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection_profile_doc(
                conn,
                &connection.provider_name,
                connection.model.as_deref(),
                connection.credential_ref.as_deref(),
            )
            .into_bytes()
        }
        Node::ConnectionMeter(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection.meter_doc(conn).into_bytes()
        }
        Node::ConnectionCapabilities(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection_capabilities_doc(
                conn,
                &connection.provider_name,
                connection.capabilities,
                connection.choice_evaluation,
                connection.decisions_supported,
                connection.decision_calibration,
                connection.generation_supported,
            )
            .into_bytes()
        }
        Node::ConnectionModels(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            let live_models = connection.live_models();
            connection_models_doc(conn, &connection.provider_name, live_models.as_deref())
                .into_bytes()
        }
        Node::Gen(_) => b"data\nevents\nctl\nstatus".to_vec(),
        Node::GenStatus(id) => {
            let g = state.gens.get(id).ok_or(ErrorCode::NotFound)?;
            generation_status_doc(id, g).into_bytes()
        }
        // clone, evaluate, decide, data, ctl, events are open/write/stream surfaces, not read here.
        _ => return Err(ErrorCode::Unsupported),
    };
    Ok(bytes)
}

/// The kind and a server-unique identity key for a node (so distinct connections
/// and operations get distinct qids).
pub(crate) fn node_identity(node: &Node) -> (FileKind, String) {
    match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::ProvidersDir => (FileKind::Dir, "providers".to_string()),
        Node::Provider(provider) => (FileKind::Dir, format!("providers/{provider}")),
        Node::ProviderModels(provider) => (FileKind::File, format!("providers/{provider}/models")),
        Node::ProviderCapabilities(provider) => {
            (FileKind::File, format!("providers/{provider}/capabilities"))
        }
        Node::ProviderStatus(provider) => (FileKind::File, format!("providers/{provider}/status")),
        Node::ConnectionsDir => (FileKind::Dir, "connections".to_string()),
        Node::Connection(c) => (FileKind::Dir, format!("connections/{c}")),
        Node::ConnectionProvider(c) => (FileKind::File, format!("connections/{c}/provider")),
        Node::ConnectionProfile(c) => (FileKind::File, format!("connections/{c}/profile")),
        Node::ConnectionMeter(c) => (FileKind::File, format!("connections/{c}/meter")),
        Node::ConnectionCapabilities(c) => {
            (FileKind::File, format!("connections/{c}/capabilities"))
        }
        Node::ConnectionModels(c) => (FileKind::File, format!("connections/{c}/models")),
        Node::Clone(c) => (FileKind::Clone, format!("connections/{c}/clone")),
        Node::Evaluate(c) => (FileKind::Clone, format!("connections/{c}/evaluate")),
        Node::Decide(c) => (FileKind::Clone, format!("connections/{c}/decide")),
        Node::Gen(id) => (FileKind::Dir, format!("gen/{id}")),
        Node::GenData(id) => (FileKind::File, format!("gen/{id}/data")),
        Node::GenEvents(id) => (FileKind::Stream, format!("gen/{id}/events")),
        Node::GenCtl(id) => (FileKind::File, format!("gen/{id}/ctl")),
        Node::GenStatus(id) => (FileKind::File, format!("gen/{id}/status")),
    }
}

pub(crate) fn hash_path(key: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

pub(crate) fn render_json_doc(value: serde_json::Value) -> String {
    let mut rendered = serde_json::to_string(&value).expect("serialize llmfs introspection doc");
    rendered.push('\n');
    rendered
}

pub(crate) fn is_writable(node: &Node) -> bool {
    matches!(
        node,
        Node::Clone(_) | Node::Evaluate(_) | Node::Decide(_) | Node::GenData(_) | Node::GenCtl(_)
    )
}

/// Whether a node has a readable surface. `data` and `ctl` are write-only sinks;
/// `clone`, `evaluate`, and `decide` are readable (the caller reads the allocated id back).
pub(crate) fn is_readable(node: &Node) -> bool {
    !matches!(node, Node::GenData(_) | Node::GenCtl(_))
}

pub(crate) fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
