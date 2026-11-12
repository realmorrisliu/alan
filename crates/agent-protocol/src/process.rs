use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{SpawnSpec, YieldKind};

/// Current wire version for `/bin/alan-agent` terminal results.
pub const AGENT_EXECUTABLE_RESULT_VERSION: u16 = 1;
const AGENT_EXECUTABLE_RESULT_RECORD_PREFIX: &[u8] = b"\n\x1ealan-agent-result-v1:";

/// File-native exec document written to `/proc/clone`.
///
/// This protocol type deliberately mirrors the Kernel document without giving
/// clients a dependency on Kernel implementation types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessExecSpec {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub namespace: ProcessNamespaceManifest,
    #[serde(default)]
    pub descriptors: BTreeMap<u32, String>,
}

/// Explicit namespace capabilities retained by a spawned Process, bound to the
/// source namespace generation from which they were selected.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessNamespaceManifest {
    pub generation: u32,
    #[serde(default)]
    pub mounts: Vec<ProcessNamespaceMount>,
}

/// One mount retained in a Process namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessNamespaceMount {
    pub path: String,
    pub access: ProcessNamespaceAccess,
}

impl ProcessNamespaceMount {
    pub fn new(path: impl Into<String>, access: ProcessNamespaceAccess) -> Self {
        Self {
            path: path.into(),
            access,
        }
    }
}

/// Access spelling used by `/proc/clone` namespace manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProcessNamespaceAccess {
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

/// `/bin/alan-agent` child-launch arguments.
///
/// `spawn` remains the authoritative launch request. `initial_task` is the
/// bounded text projection of explicitly passed parent handles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentExecutableRequest {
    pub spawn: SpawnSpec,
    pub initial_task: String,
    /// Resume an already-pending request in a recovered child instead of
    /// submitting a fresh task. Set only when reconnecting preserved child work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
    /// Execution record the child must recover before serving the resume.
    ///
    /// Only an inherited-budget reconnect sets this: the Process that yielded
    /// the request exited, so its preserved work survives only in its rollout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_rollout: Option<String>,
    /// The resume is an ancestor's authorization for an inherited stop.
    ///
    /// Without this the resumed Process cannot tell the ancestor's authorization
    /// from an empty answer, and an empty answer to an inherited stop is a
    /// rejection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inherited_resume: bool,
}

/// Terminal state emitted by `/bin/alan-agent` through `/proc/<pid>/io/output`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExecutableStatus {
    Completed,
    Paused,
    Failed,
}

/// Pending interaction retained when a child Agent Process pauses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentExecutablePause {
    pub request_id: String,
    pub kind: YieldKind,
    /// Decision details captured while the paused Process was still alive.
    ///
    /// A paused Process can be removed before its consumer looks, so anything the
    /// consumer cannot re-derive later (an inherited stop's ancestor identity)
    /// travels here as well as in the request file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Bounded terminal evidence for an Agent Executable invocation.
///
/// AgentFS remains the live process view. This record preserves the final
/// observation after Agent Runtime Service removes that live backing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentExecutableResult {
    pub version: u16,
    pub status: AgentExecutableStatus,
    #[serde(default)]
    pub output_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause: Option<AgentExecutablePause>,
}

impl AgentExecutableResult {
    pub fn completed(output_text: impl Into<String>, warnings: Vec<String>) -> Self {
        Self {
            version: AGENT_EXECUTABLE_RESULT_VERSION,
            status: AgentExecutableStatus::Completed,
            output_text: output_text.into(),
            warnings,
            error_message: None,
            pause: None,
        }
    }

    pub fn paused(
        output_text: impl Into<String>,
        warnings: Vec<String>,
        pause: AgentExecutablePause,
    ) -> Self {
        Self {
            version: AGENT_EXECUTABLE_RESULT_VERSION,
            status: AgentExecutableStatus::Paused,
            output_text: output_text.into(),
            warnings,
            error_message: None,
            pause: Some(pause),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::failed_with_output(String::new(), Vec::new(), message)
    }

    pub fn failed_with_output(
        output_text: impl Into<String>,
        warnings: Vec<String>,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        Self {
            version: AGENT_EXECUTABLE_RESULT_VERSION,
            status: AgentExecutableStatus::Failed,
            output_text: output_text.into(),
            warnings,
            error_message: Some(message),
            pause: None,
        }
    }

    /// Encode the terminal record appended after ordinary Process output.
    pub fn to_process_output_record(&self) -> Result<Vec<u8>, serde_json::Error> {
        let payload = serde_json::to_vec(self)?;
        let mut record =
            Vec::with_capacity(AGENT_EXECUTABLE_RESULT_RECORD_PREFIX.len() + payload.len());
        record.extend_from_slice(AGENT_EXECUTABLE_RESULT_RECORD_PREFIX);
        record.extend_from_slice(&payload);
        Ok(record)
    }

    /// Decode the last terminal record from a retained Process output stream.
    pub fn from_process_output(output: &[u8]) -> Result<Self, serde_json::Error> {
        let offset = output
            .windows(AGENT_EXECUTABLE_RESULT_RECORD_PREFIX.len())
            .rposition(|window| window == AGENT_EXECUTABLE_RESULT_RECORD_PREFIX)
            .ok_or_else(|| invalid_result("Agent Executable terminal record is absent"))?;
        let payload = &output[offset + AGENT_EXECUTABLE_RESULT_RECORD_PREFIX.len()..];
        let result: Self = serde_json::from_slice(payload)?;
        if result.version != AGENT_EXECUTABLE_RESULT_VERSION {
            return Err(invalid_result(
                "unsupported Agent Executable result version",
            ));
        }
        let shape_is_valid = match result.status {
            AgentExecutableStatus::Completed => {
                result.error_message.is_none() && result.pause.is_none()
            }
            AgentExecutableStatus::Paused => {
                result.error_message.is_none() && result.pause.is_some()
            }
            AgentExecutableStatus::Failed => {
                result.error_message.is_some() && result.pause.is_none()
            }
        };
        if !shape_is_valid {
            return Err(invalid_result(
                "invalid Agent Executable terminal result shape",
            ));
        }
        Ok(result)
    }
}

fn invalid_result(message: &'static str) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

/// Canonical numeric Process descriptors understood by the Agent Executable.
pub const AGENT_DEFINITION_DESCRIPTOR: u32 = 3;
pub const MEMORY_STORE_DESCRIPTOR: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SpawnLaunchInputs, SpawnRuntimeOverrides, SpawnTarget};

    #[test]
    fn exec_document_uses_kernel_wire_spellings_without_kernel_types() {
        let spec = ProcessExecSpec {
            executable: "/bin/alan-agent".to_string(),
            args: Vec::new(),
            namespace: ProcessNamespaceManifest {
                generation: 0,
                mounts: vec![ProcessNamespaceMount::new(
                    "/agent",
                    ProcessNamespaceAccess::ReadWrite,
                )],
            },
            descriptors: BTreeMap::from([(
                AGENT_DEFINITION_DESCRIPTOR,
                "/lib/agents/root".to_string(),
            )]),
        };

        let value = serde_json::to_value(spec).unwrap();
        assert_eq!(value["namespace"]["mounts"][0]["access"], "rw");
        assert_eq!(value["namespace"]["generation"], 0);
        assert_eq!(value["descriptors"]["3"], "/lib/agents/root");
        assert!(
            serde_json::from_value::<ProcessExecSpec>(serde_json::json!({
                "executable": "/bin/alan-agent",
                "args": []
            }))
            .is_err(),
            "an exec document cannot represent ambient namespace inheritance"
        );
        assert!(
            serde_json::from_value::<ProcessExecSpec>(serde_json::json!({
                "executable": "/bin/alan-agent",
                "args": [],
                "namespace": { "mounts": [] }
            }))
            .is_err(),
            "a namespace manifest must identify the snapshot generation"
        );
    }

    #[test]
    fn agent_executable_request_round_trips_spawn_spec() {
        let request = AgentExecutableRequest {
            spawn: SpawnSpec {
                target: SpawnTarget::DefinitionDescriptor {
                    descriptor: "agent-definition".to_string(),
                },
                launch: SpawnLaunchInputs {
                    task: "inspect".to_string(),
                    ..SpawnLaunchInputs::default()
                },
                handles: Vec::new(),
                host_mounts: Vec::new(),
                runtime_overrides: SpawnRuntimeOverrides::default(),
                delegated: None,
            },
            initial_task: "inspect".to_string(),
            resume: None,
            recovery_rollout: None,
            inherited_resume: false,
        };
        let encoded = serde_json::to_string(&request).unwrap();
        assert!(
            !encoded.contains("resume"),
            "a fresh launch must not carry a resume request: {encoded}"
        );
    }

    #[test]
    fn agent_executable_request_round_trips_an_explicit_resume() {
        let request = AgentExecutableRequest {
            spawn: SpawnSpec {
                target: SpawnTarget::DefinitionDescriptor {
                    descriptor: "agent-definition".to_string(),
                },
                launch: SpawnLaunchInputs {
                    task: "inspect".to_string(),
                    ..SpawnLaunchInputs::default()
                },
                handles: Vec::new(),
                host_mounts: Vec::new(),
                runtime_overrides: SpawnRuntimeOverrides::default(),
                delegated: None,
            },
            initial_task: "inspect".to_string(),
            resume: Some("inherited_budget-1".to_string()),
            recovery_rollout: None,
            inherited_resume: true,
        };

        let encoded = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<AgentExecutableRequest>(&encoded).unwrap(),
            request
        );
    }

    #[test]
    fn paused_agent_executable_result_preserves_terminal_observation() {
        let result = AgentExecutableResult::paused(
            "partial",
            vec!["warning".to_string()],
            AgentExecutablePause {
                request_id: "request-1".to_string(),
                kind: YieldKind::Confirmation,
                // An inherited stop's identity must survive here: the paused
                // Process backing is gone before its consumer looks.
                payload: Some(serde_json::json!({
                    "ancestor_task_id": "ancestor-1",
                    "dimension": "model_calls",
                })),
            },
        );

        let encoded = serde_json::to_vec(&result).unwrap();
        assert_eq!(
            serde_json::from_slice::<AgentExecutableResult>(&encoded).unwrap(),
            result
        );
        let mut process_output = b"arbitrary assistant output".to_vec();
        process_output.extend_from_slice(&result.to_process_output_record().unwrap());
        let decoded = AgentExecutableResult::from_process_output(&process_output).unwrap();
        assert_eq!(decoded, result);
        assert_eq!(
            decoded.pause.unwrap().payload.unwrap()["ancestor_task_id"],
            serde_json::json!("ancestor-1"),
            "the pause record must carry the inherited identity through the process output"
        );

        assert!(
            AgentExecutableResult::from_process_output(&serde_json::to_vec(&result).unwrap())
                .is_err()
        );
        let mut invalid = result;
        invalid.version += 1;
        assert!(
            AgentExecutableResult::from_process_output(
                &invalid.to_process_output_record().unwrap()
            )
            .is_err()
        );
        invalid.version = AGENT_EXECUTABLE_RESULT_VERSION;
        invalid.pause = None;
        assert!(
            AgentExecutableResult::from_process_output(
                &invalid.to_process_output_record().unwrap()
            )
            .is_err()
        );
    }
}
