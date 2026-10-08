//! State-owned traversal and file materialization for the AgentFS surface.

use alan_ap::{ErrorCode, Fid, FileKind, Qid, Stream};
use alan_knowledge::{ContentHash, RootAccess};

use super::{
    ACTIONS_HELP, MACHINE_CTL_HELP, Node, State, TAPE_ROOT_NAME, action_output_root_name,
    map_knowledge_error,
};

impl State {
    pub(super) fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    /// The qid for `node`, with its current version from the table.
    pub(super) fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    /// Record that `node`'s content changed: bump its qid version.
    pub(super) fn bump(&mut self, node: &Node) {
        let (_, path) = node_identity(node);
        self.versions.bump(path);
    }

    pub(super) fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "io" => Ok(Node::IoDir),
                "machine" => Ok(Node::MachineDir),
                "events" => Ok(Node::Events),
                "requests" => Ok(Node::RequestsDir),
                "actions" => Ok(Node::ActionsDir),
                "context" => Ok(Node::ContextDir),
                "children" => Ok(Node::ChildrenDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::IoDir => match name {
                "input" => Ok(Node::Input),
                "output" => Ok(Node::Output),
                "events" => Ok(Node::IoEvents),
                _ => Err(ErrorCode::NotFound),
            },
            Node::MachineDir => match name {
                "tape" => Ok(Node::Tape),
                "status" => Ok(Node::Status),
                "evaluation" => Ok(Node::Evaluation),
                "ctl" => Ok(Node::MachineCtl),
                "ui" => Ok(Node::UiDir),
                "checkpoints" => Ok(Node::CheckpointsDir),
                _ => Err(ErrorCode::NotFound),
            },
            Node::UiDir => match name {
                "skills" => Ok(Node::UiSkills),
                "models" => Ok(Node::UiModels),
                "queue" => Ok(Node::UiQueue),
                "activity" => Ok(Node::UiActivity),
                "plan" => Ok(Node::UiPlan),
                "thinking" => Ok(Node::UiThinking),
                "notice" => Ok(Node::UiNotice),
                "events" => Ok(Node::UiEvents),
                _ => Err(ErrorCode::NotFound),
            },
            Node::CheckpointsDir => match name {
                "current" => Ok(Node::CurrentCheckpoint),
                _ => Err(ErrorCode::NotFound),
            },
            Node::RequestsDir => match name {
                "clone" => Ok(Node::RequestsClone),
                "events" => Ok(Node::RequestsEvents),
                id if self.requests.contains_key(id) => Ok(Node::Request(id.to_string())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Request(id) => match name {
                "kind" => Ok(Node::RequestField(id.clone(), "kind")),
                "prompt" => Ok(Node::RequestField(id.clone(), "prompt")),
                "options" => Ok(Node::RequestField(id.clone(), "options")),
                "status" => Ok(Node::RequestField(id.clone(), "status")),
                "ctl" => Ok(Node::RequestField(id.clone(), "ctl")),
                "response" => Ok(Node::RequestField(id.clone(), "response")),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ActionsDir => match name {
                "help" => Ok(Node::ActionsHelp),
                "clone" => Ok(Node::ActionsClone),
                "events" => Ok(Node::ActionsEvents),
                id if self.actions.contains_key(id) => Ok(Node::Action(id.to_string())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::Action(id) => match name {
                "name" => Ok(Node::ActionField(id.clone(), "name")),
                "status" => Ok(Node::ActionField(id.clone(), "status")),
                "output" => Ok(Node::ActionField(id.clone(), "output")),
                "result" => Ok(Node::ActionField(id.clone(), "result")),
                "approval" => Ok(Node::ActionField(id.clone(), "approval")),
                "process" => Ok(Node::ActionField(id.clone(), "process")),
                _ => Err(ErrorCode::NotFound),
            },
            // context/ and children/ are agent-layout dirs, empty until the engine
            // projects into them — any child is simply absent for now.
            Node::ContextDir | Node::ChildrenDir => Err(ErrorCode::NotFound),
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    pub(super) fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"io\nmachine\nevents\nrequests\nactions\ncontext\nchildren".to_vec(),
            Node::ContextDir | Node::ChildrenDir => Vec::new(),
            Node::IoDir => b"input\noutput\nevents".to_vec(),
            Node::MachineDir => b"tape\nstatus\nevaluation\nctl\nui\ncheckpoints".to_vec(),
            Node::UiDir => {
                b"activity\nplan\nthinking\nnotice\nevents\nqueue\nmodels\nskills".to_vec()
            }
            Node::CheckpointsDir => b"current".to_vec(),
            Node::CurrentCheckpoint => format!("{}\n", self.tape_root).into_bytes(),
            Node::Status => self.status.clone().into_bytes(),
            Node::Evaluation => self.evaluation.clone(),
            Node::UiSkills => self.ui_skills.clone().into_bytes(),
            Node::UiModels => self.ui_models.clone().into_bytes(),
            Node::UiQueue => self.ui_queue.clone().into_bytes(),
            Node::UiActivity => self.ui_activity.clone().into_bytes(),
            Node::UiPlan => self.ui_plan.clone().into_bytes(),
            Node::UiThinking => self.ui_thinking.clone().into_bytes(),
            Node::UiNotice => self.ui_notice.clone().into_bytes(),
            // machine/ctl exposes its accepted commands in-band, so a
            // namespace-native client discovers them by reading the file rather
            // than from external docs (self-describing namespace).
            Node::MachineCtl => MACHINE_CTL_HELP.as_bytes().to_vec(),
            Node::RequestsDir => listing(&["clone", "events"], self.requests.keys()),
            Node::ActionsDir => listing(&["clone", "events", "help"], self.actions.keys()),
            Node::ActionsHelp => ACTIONS_HELP.as_bytes().to_vec(),
            Node::Request(_) => b"kind\nprompt\noptions\nstatus\nresponse\nctl".to_vec(),
            Node::Action(_) => b"name\nstatus\noutput\nresult\napproval\nprocess".to_vec(),
            Node::RequestField(id, field) => {
                let r = self.requests.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "ctl" => return Ok(b"cancel\n".to_vec()),
                    "kind" => &r.kind,
                    "prompt" => &r.prompt,
                    "options" => &r.options,
                    "status" => &r.status,
                    _ => &r.response,
                }
                .clone()
                .into_bytes()
            }
            Node::ActionField(id, field) => {
                let a = self.actions.get(id).ok_or(ErrorCode::NotFound)?;
                match *field {
                    "name" => a.name.clone().into_bytes(),
                    "status" => a.status.clone().into_bytes(),
                    "result" => a.result.clone().into_bytes(),
                    "approval" => a.approval.clone().into_bytes(),
                    "process" => a.process.clone().into_bytes(),
                    "output" if a.output_retention_expired => a.output.clone().into_bytes(),
                    "output" if a.output_root.is_some() => self
                        .knowledge
                        .root(&action_output_root_name(id))
                        .map_err(map_knowledge_error)
                        .and_then(|root| {
                            self.knowledge
                                .read_bound_root(&root)
                                .map_err(map_knowledge_error)
                        })?,
                    "output" => a.output.clone().into_bytes(),
                    _ => return Err(ErrorCode::NotFound),
                }
            }
            // Streams are served via stream_for; clone files via the fid's clone_id.
            Node::Input
            | Node::Output
            | Node::IoEvents
            | Node::Tape
            | Node::Events
            | Node::UiEvents
            | Node::RequestsEvents
            | Node::ActionsEvents => {
                return Err(ErrorCode::Unsupported);
            }
            Node::RequestsClone | Node::ActionsClone => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    pub(super) fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Output => Some(self.output.clone()),
            Node::Input => Some(self.input.clone()),
            Node::Tape => Some(self.tape.clone()),
            Node::Events => Some(self.events.clone()),
            // io/events is IO-scoped; the per-container streams are their own.
            Node::IoEvents => Some(self.io_events.clone()),
            Node::UiEvents => Some(self.ui_events.clone()),
            Node::RequestsEvents => Some(self.request_events.clone()),
            Node::ActionsEvents => Some(self.action_events.clone()),
            _ => None,
        }
    }

    pub(super) fn append_tape_block(&mut self, data: &[u8]) -> Result<(), ErrorCode> {
        let root = self
            .knowledge
            .fork_append_bytes(&self.tape_root, [data])
            .map_err(map_knowledge_error)?;
        self.knowledge
            .bind_root(TAPE_ROOT_NAME, root.clone(), RootAccess::ReadWrite)
            .map_err(map_knowledge_error)?;
        self.tape_root = root;
        self.bump(&Node::CurrentCheckpoint);
        Ok(())
    }

    pub(super) fn materialized_tape(&self) -> Result<Vec<u8>, ErrorCode> {
        let root = self
            .knowledge
            .root(TAPE_ROOT_NAME)
            .map_err(map_knowledge_error)?;
        self.knowledge
            .read_bound_root(&root)
            .map_err(map_knowledge_error)
    }

    pub(super) fn store_action_output(
        &mut self,
        action_id: &str,
        bytes: &[u8],
    ) -> Result<ContentHash, ErrorCode> {
        let root = self
            .knowledge
            .checkpoint_from_bytes([bytes])
            .map_err(map_knowledge_error)?;
        self.knowledge
            .bind_root(
                action_output_root_name(action_id),
                root.clone(),
                RootAccess::ReadWrite,
            )
            .map_err(map_knowledge_error)?;
        Ok(root)
    }
}

// Wire shape only; canonical Skill authority belongs to Engine.
pub(super) const UNKNOWN_SKILLS: &str = r#"{"version":1,"publication_version":0,"process_path":"","known":false,"mentionable_skill_ids":[]}"#;

pub(super) fn skill_projection(value: &str) -> String {
    use serde_json::Value;
    let safe = serde_json::from_str::<Value>(value).ok().filter(|value| {
        let Some(fields) = value.as_object() else {
            return false;
        };
        let keys = [
            "version",
            "publication_version",
            "process_path",
            "known",
            "mentionable_skill_ids",
        ];
        fields.len() == keys.len()
            && keys.iter().all(|key| fields.contains_key(*key))
            && value["version"].as_u64() == Some(1)
            && value["publication_version"].as_u64().is_some()
            && value["process_path"].is_string()
            && value["known"].is_boolean()
            && value["mentionable_skill_ids"]
                .as_array()
                .is_some_and(|ids| {
                    ids.iter().all(Value::is_string) && (value["known"] == true || ids.is_empty())
                })
    });
    safe.map(|value| value.to_string())
        .unwrap_or_else(|| UNKNOWN_SKILLS.to_string())
}

pub(super) fn is_writable(node: &Node) -> bool {
    match node {
        Node::Input
        | Node::Output
        | Node::Tape
        | Node::MachineCtl
        | Node::UiModels
        | Node::UiQueue
        | Node::UiActivity
        | Node::UiPlan
        | Node::UiThinking
        | Node::UiSkills
        | Node::UiNotice
        | Node::UiEvents
        | Node::RequestsClone
        | Node::ActionsClone
        | Node::ActionField(..) => true,
        // Request status and machine status are read-only observations.
        Node::RequestField(_, field) => *field != "status",
        _ => false,
    }
}

/// A directory listing joining fixed entries with dynamic ids.
fn listing<'a>(fixed: &[&str], ids: impl Iterator<Item = &'a String>) -> Vec<u8> {
    let mut names: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
    names.extend(ids.cloned());
    names.join("\n").into_bytes()
}

/// A node's stable identity: its file kind and a server-unique qid path, keyed by
/// its full file identity so distinct files (and distinct request/action ids)
/// never share a qid. The qid *version* is layered on top from the state's
/// [`alan_ap::VersionTable`] (see [`State::qid`]); this part never changes for a node.
pub(super) fn node_identity(node: &Node) -> (FileKind, u64) {
    use std::hash::{Hash, Hasher};
    fn path_of(key: &str) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        h.finish()
    }
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::IoDir => (FileKind::Dir, "io".into()),
        Node::MachineDir => (FileKind::Dir, "machine".into()),
        Node::UiDir => (FileKind::Dir, "machine/ui".into()),
        Node::CheckpointsDir => (FileKind::Dir, "machine/checkpoints".into()),
        Node::RequestsDir => (FileKind::Dir, "requests".into()),
        Node::ActionsDir => (FileKind::Dir, "actions".into()),
        Node::ActionsHelp => (FileKind::File, "actions/help".into()),
        Node::ContextDir => (FileKind::Dir, "context".into()),
        Node::ChildrenDir => (FileKind::Dir, "children".into()),
        Node::Request(id) => (FileKind::Dir, format!("requests/{id}")),
        Node::Action(id) => (FileKind::Dir, format!("actions/{id}")),
        Node::RequestsClone => (FileKind::Clone, "requests/clone".into()),
        Node::ActionsClone => (FileKind::Clone, "actions/clone".into()),
        Node::RequestsEvents => (FileKind::Stream, "requests/events".into()),
        Node::ActionsEvents => (FileKind::Stream, "actions/events".into()),
        Node::Input => (FileKind::Stream, "io/input".into()),
        Node::Output => (FileKind::Stream, "io/output".into()),
        Node::IoEvents => (FileKind::Stream, "io/events".into()),
        Node::Tape => (FileKind::Stream, "machine/tape".into()),
        Node::Events => (FileKind::Stream, "events".into()),
        Node::UiEvents => (FileKind::Stream, "machine/ui/events".into()),
        Node::Status => (FileKind::File, "machine/status".into()),
        Node::Evaluation => (FileKind::File, "machine/evaluation".into()),
        Node::MachineCtl => (FileKind::File, "machine/ctl".into()),
        Node::UiSkills => (FileKind::File, "machine/ui/skills".into()),
        Node::UiModels => (FileKind::File, "machine/ui/models".into()),
        Node::UiQueue => (FileKind::File, "machine/ui/queue".into()),
        Node::UiActivity => (FileKind::File, "machine/ui/activity".into()),
        Node::UiPlan => (FileKind::File, "machine/ui/plan".into()),
        Node::UiThinking => (FileKind::File, "machine/ui/thinking".into()),
        Node::UiNotice => (FileKind::File, "machine/ui/notice".into()),
        Node::CurrentCheckpoint => (FileKind::File, "machine/checkpoints/current".into()),
        Node::RequestField(id, field) => (FileKind::File, format!("requests/{id}/{field}")),
        Node::ActionField(id, field) => (FileKind::File, format!("actions/{id}/{field}")),
    };
    (kind, path_of(&key))
}
