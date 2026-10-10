//! Tool package discovery, capability resolution, and Process-backed execution.

use anyhow::{Context, Result, bail};
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceActionRecord, NamespaceToolActionEvidence, NamespaceToolActionOutput,
    NamespaceToolExecution, NamespaceToolProcessError, client::NamespaceClient,
    process_files::NamespaceProcessResult,
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

    pub(crate) fn default_cwd(&self) -> Option<std::path::PathBuf> {
        self.execution_binding()
            .map(|binding| binding.namespace_cwd)
    }

    fn read_only_context(
        &self,
        name: &str,
        evidence: NamespaceToolActionEvidence<'_>,
    ) -> Option<alan_agent_protocol::ActionReadOnlyContext> {
        if evidence.approval != "not_required" {
            return None;
        }
        let context = self.tool_process_context.as_ref()?;
        let value = alan_agent_protocol::ActionReadOnlyContext {
            owner: format!("/agent/{}", context.pid),
            submission: evidence.submission_id?.into(),
            authority: context.tool_runner.read_only_presentation_scope(
                context.pid,
                name,
                evidence.arguments,
            )?,
        };
        value.is_valid().then_some(value)
    }

    pub(crate) fn change_process_directory(
        &self,
        path: &std::path::Path,
    ) -> Result<std::path::PathBuf> {
        let context = self
            .tool_process_context
            .as_ref()
            .context("Process has no Tool execution context")?;
        context
            .tool_runner
            .change_process_directory(context.pid, path)
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
        self.run_action_with_cancel_and_timeout(tool_name, None, executable, args, &cancel, 30)
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
        self.run_action_with_cancel_and_timeout(tool_name, None, executable, args, cancel, 30)
            .await
    }

    pub(crate) async fn run_action_with_cancel_and_timeout<I, S>(
        &self,
        tool_name: &str,
        evidence: Option<NamespaceToolActionEvidence<'_>>,
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
        let read_only_before =
            evidence.and_then(|evidence| self.read_only_context(tool_name, evidence));
        let pid = self.process_files.spawn_process(executable, args).await?;
        let result = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.process_files.write_process_control_for_pid(&pid, "cancel").await;
                return Err(NamespaceToolProcessError {
                    category: super::NamespaceToolObservationFailure::Cancelled,
                    pid,
                }.into());
            }
            result = self.process_files.read_process_result(&pid, timeout_secs) => {
                match result {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = self
                            .process_files
                            .write_process_control_for_pid(&pid, "cancel")
                            .await;
                        return Err(NamespaceToolProcessError {
                            category: if err.downcast_ref::<tokio::time::error::Elapsed>().is_some() {
                                super::NamespaceToolObservationFailure::Timeout
                            } else {
                                super::NamespaceToolObservationFailure::ResultUnavailable
                            },
                            pid,
                        }.into());
                    }
                }
            }
        };
        let action_exit_code = logical_tool_action_exit_code(&result);
        let native_receipt = self.tool_process_context.as_ref().and_then(|context| {
            context
                .tool_runner
                .take_read_only_receipt(pid.parse().ok()?, context.pid, tool_name)
        });
        let action_status = if action_exit_code == 0 {
            "completed"
        } else {
            "failed"
        };
        let mut result_doc = serde_json::json!({
            "exit_code": action_exit_code,
        });
        if let Some(evidence) = evidence {
            result_doc["call_id"] = serde_json::json!(evidence.call_id);
            let payload = crate::runtime::tool_execution::namespace_tool_payload(
                NamespaceToolActionOutput {
                    action_id: String::new(),
                    pid: pid.clone(),
                    output: result.output.clone(),
                    exit_code: action_exit_code,
                },
            )?;
            crate::runtime::tool_presentation::write_action_metadata(
                &mut result_doc,
                tool_name,
                evidence.arguments,
                &payload,
            )?;
            if action_exit_code == 0
                && executable == format!("/bin/{tool_name}")
                && let Some(context) = read_only_before
                && native_receipt.as_ref() == Some(&context.authority)
                && self.read_only_context(tool_name, evidence).as_ref() == Some(&context)
            {
                result_doc["read_only_context"] = serde_json::to_value(context)?;
            }
        }
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
                    .with_approval(evidence.map_or("not_required", |evidence| evidence.approval))
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
