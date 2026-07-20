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
    #[serde(default)]
    pub namespace: Option<ProcessNamespaceManifest>,
    #[serde(default)]
    pub descriptors: BTreeMap<u32, String>,
}

/// Explicit namespace capabilities retained by a spawned Process.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessNamespaceManifest {
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
            namespace: Some(ProcessNamespaceManifest {
                mounts: vec![ProcessNamespaceMount::new(
                    "/agent",
                    ProcessNamespaceAccess::ReadWrite,
                )],
            }),
            descriptors: BTreeMap::from([(
                AGENT_DEFINITION_DESCRIPTOR,
                "/lib/agents/root".to_string(),
            )]),
        };

        let value = serde_json::to_value(spec).unwrap();
        assert_eq!(value["namespace"]["mounts"][0]["access"], "rw");
        assert_eq!(value["descriptors"]["3"], "/lib/agents/root");
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
            },
        );

        let encoded = serde_json::to_vec(&result).unwrap();
        assert_eq!(
            serde_json::from_slice::<AgentExecutableResult>(&encoded).unwrap(),
            result
        );
        let mut process_output = b"arbitrary assistant output".to_vec();
        process_output.extend_from_slice(&result.to_process_output_record().unwrap());
        assert_eq!(
            AgentExecutableResult::from_process_output(&process_output).unwrap(),
            result
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
