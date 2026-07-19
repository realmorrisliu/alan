//! Tool package discovery, capability resolution, and Process-backed execution.

use anyhow::{Context, Result, bail};
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceActionRecord, NamespaceToolActionOutput, NamespaceToolExecution,
    client::NamespaceClient, process_files::NamespaceProcessResult,
};
use crate::{evidence::redact_durable_evidence_text, runtime::ToolPackageManifest};

impl NamespaceToolExecution {
    fn client(&self) -> NamespaceClient {
        NamespaceClient::new(self.root.clone())
    }

    pub(crate) fn execution_binding(&self) -> Option<crate::tools::ToolExecutionBinding> {
        let context = self.tool_process_context.as_ref()?;
        context.tool_runner.process_binding(context.pid)
    }

    pub(crate) fn resolve_capability(
        &self,
        package: &ToolPackageManifest,
        arguments: &serde_json::Value,
    ) -> alan_agent_protocol::ToolCapability {
        if !package.capability_is_argument_dependent {
            return package.capability;
        }
        self.tool_process_context
            .as_ref()
            .and_then(|context| {
                context
                    .tool_runner
                    .capability_for_tool(&package.name, arguments)
            })
            .unwrap_or(alan_agent_protocol::ToolCapability::Unknown)
    }

    #[cfg(test)]
    pub(crate) fn set_execution_binding(
        &self,
        binding: crate::tools::ToolExecutionBinding,
    ) -> bool {
        self.tool_process_context.as_ref().is_some_and(|context| {
            context
                .tool_runner
                .register_process_binding(context.pid, binding);
            true
        })
    }

    #[cfg(test)]
    pub(crate) fn sandbox_writable_roots(&self) -> Vec<std::path::PathBuf> {
        self.execution_binding()
            .and_then(|binding| binding.sandbox_spec)
            .map(|spec| spec.writable_roots)
            .unwrap_or_default()
    }

    /// Discover model-callable Tools from complete packages visible in this namespace.
    pub(crate) async fn discover_packages(&self) -> Result<Vec<ToolPackageManifest>> {
        let client = self.client();
        let mut packages = Vec::new();
        for name in client
            .try_read_directory_names("/bin")
            .await?
            .unwrap_or_default()
        {
            if name.is_empty() || name.contains('/') {
                continue;
            }
            let path = format!("/lib/exec/{name}/manifest");
            let Some(raw) = client.try_read_file(&path).await? else {
                continue;
            };
            let manifest: ToolPackageManifest = serde_json::from_slice(&raw)
                .with_context(|| format!("parse Tool manifest at {path}"))?;
            manifest.validate_for_name(&name)?;
            packages.push(manifest);
        }
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(packages)
    }

    #[cfg(test)]
    pub(crate) async fn run_action<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let cancel = CancellationToken::new();
        self.run_action_with_cancel_and_timeout(tool_name, executable, args, &cancel, 30)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn run_action_with_cancel<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.run_action_with_cancel_and_timeout(tool_name, executable, args, cancel, 30)
            .await
    }

    pub(crate) async fn run_action_with_cancel_and_timeout<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
        timeout_secs: usize,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if cancel.is_cancelled() {
            bail!("tool process cancelled before spawn");
        }
        let pid = self.process_files.spawn_process(executable, args).await?;
        let result = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.process_files.write_process_control_for_pid(&pid, "cancel").await;
                bail!("tool process {pid} cancelled");
            }
            result = self.process_files.read_process_result(&pid, timeout_secs) => {
                match result {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = self
                            .process_files
                            .write_process_control_for_pid(&pid, "cancel")
                            .await;
                        return Err(err).with_context(|| {
                            format!("read tool process {pid} result")
                        });
                    }
                }
            }
        };
        let action_exit_code = logical_tool_action_exit_code(&result);
        let action_status = if action_exit_code == 0 {
            "completed"
        } else {
            "failed"
        };
        let mut result_doc = serde_json::json!({
            "exit_code": action_exit_code,
        });
        if action_exit_code != result.exit_code
            && let Some(object) = result_doc.as_object_mut()
        {
            object.insert(
                "process_exit_code".to_string(),
                serde_json::json!(result.exit_code),
            );
        }
        let durable_output = redact_durable_evidence_text(&result.output);
        let action_id = self
            .agent_files
            .write_action(
                NamespaceActionRecord::new(tool_name, action_status)
                    .with_output(durable_output.text)
                    .with_result(result_doc.to_string())
                    .with_approval("not_required")
                    .with_process(format!("/proc/{pid}")),
            )
            .await?;
        Ok(NamespaceToolActionOutput {
            action_id,
            pid,
            output: result.output,
            exit_code: action_exit_code,
        })
    }
}

fn logical_tool_action_exit_code(result: &NamespaceProcessResult) -> i32 {
    let trimmed = result.output.trim();
    if trimmed.is_empty() {
        return result.exit_code;
    }

    let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return result.exit_code;
    };
    let payload_exit_code = payload
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
        .and_then(|code| i32::try_from(code).ok());
    let payload_success = payload.get("success").and_then(serde_json::Value::as_bool);

    if matches!(payload_success, Some(false)) {
        return payload_exit_code
            .filter(|code| *code != 0)
            .unwrap_or(if result.exit_code != 0 {
                result.exit_code
            } else {
                1
            });
    }

    if let Some(exit_code) = payload_exit_code
        && exit_code != 0
    {
        return exit_code;
    }

    result.exit_code
}
