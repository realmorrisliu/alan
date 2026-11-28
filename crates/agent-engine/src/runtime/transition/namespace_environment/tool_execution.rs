//! Tool package discovery, capability resolution, and Process-backed execution.

use std::sync::Arc;

use alan_agent_protocol::ToolResultPresentation;
use anyhow::{Context, Result, bail};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::{
    NamespaceActionRecord, NamespaceAgentFiles, NamespaceToolActionEvidence,
    NamespaceToolActionOutput, NamespaceToolExecution, NamespaceToolProcessError,
    agent_files::PendingActionPublication, client::NamespaceClient,
    process_files::NamespaceProcessResult,
};
use crate::{evidence::redact_durable_evidence_text, runtime::ToolPackageManifest};

/// A completed Tool Process ready to be published to AgentFS `actions/` in deterministic order.
pub(crate) struct PreparedToolAction {
    pid: String,
    output: String,
    exit_code: i32,
    action_record: NamespaceActionRecord,
}

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

    /// Revalidate requested child Host Mount targets against live `ToolExecutionAuthority`
    /// projections when an authoritative HostMountService is attached.
    pub(crate) fn validate_live_host_mounts(
        &self,
        mounts: &[alan_agent_protocol::SpawnHostMount],
    ) -> Option<String> {
        if mounts.is_empty() {
            return None;
        }
        let context = self.tool_process_context.as_ref()?;
        context.tool_runner.authority_revision(context.pid)?;
        let active_roots = context.tool_runner.project_knowledge_roots(context.pid);
        for mount in mounts {
            if !active_roots
                .iter()
                .any(|root| mount.target.starts_with(root))
            {
                return Some(format!(
                    "Host Mount grant '{}' for '{}' is no longer active on the parent Process.",
                    mount.grant,
                    mount.target.display()
                ));
            }
        }
        None
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

    /// Whether the Tool declares this exact invocation safe to repeat.
    ///
    /// The lookup is namespace-scoped: it resolves the same registered Tool the
    /// Process would execute, never a package-supplied capability claim.
    pub(crate) fn allows_idempotent_retry(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> bool {
        self.tool_process_context.as_ref().is_some_and(|context| {
            context
                .tool_runner
                .allows_idempotent_retry_for_tool(tool_name, arguments)
        })
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
        let streaming = if tool_name == "bash"
            && let Some(ev) = evidence
            && let Some(context) = &self.tool_process_context
        {
            self.start_streaming_bash_action(context, ev).await?
        } else {
            None
        };
        let preallocated_action_id = streaming.as_ref().map(|stream| stream.action_id.as_str());
        let prepared = self
            .execute_action_process_with_action_id(
                tool_name,
                evidence,
                executable,
                args,
                cancel,
                timeout_secs,
                preallocated_action_id,
            )
            .await;
        match (streaming, prepared) {
            (Some(stream), Ok(prepared)) => {
                let action_id = stream.finish().await;
                let mut pending =
                    PendingActionPublication::with_retained_id(prepared.action_record, action_id);
                let action_id = self.agent_files.publish_action(&mut pending).await?;
                Ok(NamespaceToolActionOutput {
                    action_id,
                    pid: prepared.pid,
                    output: prepared.output,
                    exit_code: prepared.exit_code,
                })
            }
            (Some(stream), Err(err)) => {
                let call_id = evidence.map(|ev| ev.call_id).unwrap_or("");
                let cmdline = evidence
                    .and_then(|ev| {
                        ev.arguments
                            .get("command")
                            .and_then(serde_json::Value::as_str)
                    })
                    .unwrap_or("");
                let approval = evidence.map_or("not_required", |ev| ev.approval);
                stream
                    .abort_failed(&self.agent_files, call_id, cmdline, approval, &err)
                    .await;
                Err(err)
            }
            (None, Ok(prepared)) => self.publish_prepared_action(prepared).await,
            (None, Err(err)) => Err(err),
        }
    }

    async fn start_streaming_bash_action(
        &self,
        context: &super::NamespaceToolProcessContext,
        evidence: NamespaceToolActionEvidence<'_>,
    ) -> Result<Option<StreamingBashAction>> {
        let client = self.client();
        let agent_path = self.agent_files.agent_path().to_string();
        let mut retained = None;
        client
            .clone_via_open_retained(&format!("{agent_path}/actions/clone"), &mut retained)
            .await
            .context("pre-allocate streaming bash action")?;
        let action_id = retained.context("missing cloned action id")?;
        let action_path = format!("{agent_path}/actions/{action_id}");
        let call_id = evidence.call_id.to_string();
        let cmdline = evidence
            .arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let initial_result = build_live_bash_result_doc(&call_id, &cmdline, "", "", false);
        client
            .write_document(&format!("{action_path}/name"), b"bash")
            .await?;
        client
            .write_document(
                &format!("{action_path}/approval"),
                evidence.approval.as_bytes(),
            )
            .await?;
        client
            .write_document(&format!("{action_path}/result"), initial_result.as_bytes())
            .await?;

        let (tx, mut rx) = watch::channel((String::new(), String::new(), false));
        let sink: crate::tools::OutputProgressCallback = Arc::new(
            move |stdout_tail: &str, stderr_tail: &str, truncated: bool| {
                let _ =
                    tx.send_replace((stdout_tail.to_string(), stderr_tail.to_string(), truncated));
            },
        );
        context.tool_runner.set_progress_sink(context.pid, sink);

        let writer_client = self.client();
        let result_path = format!("{action_path}/result");
        let cmdline_for_task = cmdline.clone();
        let call_id_for_task = call_id.clone();
        let stop_token = CancellationToken::new();
        let task_stop = stop_token.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    changed = rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                    }
                    _ = task_stop.cancelled() => {
                        if rx.has_changed().unwrap_or(false) {
                            let (stdout_tail, stderr_tail, truncated) = rx.borrow_and_update().clone();
                            let doc = build_live_bash_result_doc(
                                &call_id_for_task,
                                &cmdline_for_task,
                                &stdout_tail,
                                &stderr_tail,
                                truncated,
                            );
                            let _ = writer_client.write_document(&result_path, doc.as_bytes()).await;
                        }
                        break;
                    }
                }
                let (stdout_tail, stderr_tail, truncated) = rx.borrow_and_update().clone();
                let doc = build_live_bash_result_doc(
                    &call_id_for_task,
                    &cmdline_for_task,
                    &stdout_tail,
                    &stderr_tail,
                    truncated,
                );
                if let Err(err) = writer_client
                    .write_document(&result_path, doc.as_bytes())
                    .await
                {
                    tracing::debug!(error = %err, "failed to write live bash action preview");
                    break;
                }
            }
        });

        Ok(Some(StreamingBashAction {
            action_id,
            tool_runner: context.tool_runner.clone(),
            pid: context.pid,
            stop_token,
            task: Some(task),
        }))
    }

    /// Spawn and wait for one Tool Process without publishing its `actions/` record yet.
    /// Callers running read-only batches concurrently use this to overlap execution and then
    /// commit `actions/` in deterministic batch order via [`Self::publish_prepared_action`].
    pub(crate) async fn execute_action_process<I, S>(
        &self,
        tool_name: &str,
        evidence: Option<NamespaceToolActionEvidence<'_>>,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
        timeout_secs: usize,
    ) -> Result<PreparedToolAction>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.execute_action_process_with_action_id(
            tool_name,
            evidence,
            executable,
            args,
            cancel,
            timeout_secs,
            None,
        )
        .await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "internal helper carries optional preallocated streaming action id"
    )]
    async fn execute_action_process_with_action_id<I, S>(
        &self,
        tool_name: &str,
        evidence: Option<NamespaceToolActionEvidence<'_>>,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
        timeout_secs: usize,
        preallocated_action_id: Option<&str>,
    ) -> Result<PreparedToolAction>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if cancel.is_cancelled() {
            bail!("tool process cancelled before spawn");
        }
        let mut operation = self.operation_cleanup.lease(self.client(), b"cancel")?;
        let read_only_before =
            evidence.and_then(|evidence| self.read_only_context(tool_name, evidence));
        let deadline = async {
            if timeout_secs == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(timeout_secs as u64)).await;
            }
        };
        tokio::pin!(deadline);
        let mut retained = None;
        let mut category = super::NamespaceToolObservationFailure::ResultUnavailable;
        let allocation = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                category = super::NamespaceToolObservationFailure::Cancelled;
                Err(anyhow::anyhow!("Tool allocation cancelled"))
            },
            _ = &mut deadline => {
                category = super::NamespaceToolObservationFailure::Timeout;
                Err(anyhow::anyhow!("Tool allocation timed out"))
            },
            result = self.process_files.spawn_process(executable, args, &mut retained) => result,
        };
        let allocation = if let Some(pid) = &retained {
            operation.capture(format!("/proc/{pid}/ctl"))?;
            allocation.map_err(|error| {
                error.context(NamespaceToolProcessError {
                    category,
                    pid: pid.clone(),
                    action_id: preallocated_action_id.map(str::to_string),
                })
            })
        } else {
            allocation
        };
        let pid = operation.retire_on_error(allocation).await?;
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(super::NamespaceToolObservationFailure::Cancelled),
            _ = &mut deadline => Err(super::NamespaceToolObservationFailure::Timeout),
            result = self.process_files.read_process_result(&pid, timeout_secs) => {
                result.map_err(|err| {
                    if err.downcast_ref::<tokio::time::error::Elapsed>().is_some() {
                        super::NamespaceToolObservationFailure::Timeout
                    } else {
                        super::NamespaceToolObservationFailure::ResultUnavailable
                    }
                })
            }
        };
        let result = match result {
            Ok(result) => result,
            Err(category) => {
                let error = anyhow::Error::new(NamespaceToolProcessError {
                    category,
                    pid,
                    action_id: preallocated_action_id.map(str::to_string),
                });
                return Err(match operation.retire().await {
                    Ok(()) => error,
                    Err(cleanup) => error.context(cleanup),
                });
            }
        };
        operation.settled();
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
        let action_record = NamespaceActionRecord::new(tool_name, action_status)
            .with_output(durable_output.text)
            .with_result(result_doc.to_string())
            .with_approval(evidence.map_or("not_required", |evidence| evidence.approval))
            .with_process(format!("/proc/{pid}"));
        Ok(PreparedToolAction {
            pid,
            output: result.output,
            exit_code: action_exit_code,
            action_record,
        })
    }

    /// Publish a completed Tool Process to AgentFS `actions/`.
    pub(crate) async fn publish_prepared_action(
        &self,
        prepared: PreparedToolAction,
    ) -> Result<NamespaceToolActionOutput> {
        let action_id = self
            .agent_files
            .write_action(prepared.action_record)
            .await?;
        Ok(NamespaceToolActionOutput {
            action_id,
            pid: prepared.pid,
            output: prepared.output,
            exit_code: prepared.exit_code,
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

struct StreamingBashAction {
    action_id: String,
    tool_runner: crate::tools::ToolProcessRunner,
    pid: u64,
    stop_token: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for StreamingBashAction {
    fn drop(&mut self) {
        self.tool_runner.clear_progress_sink(self.pid);
        self.stop_token.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl StreamingBashAction {
    async fn finish(mut self) -> String {
        self.tool_runner.clear_progress_sink(self.pid);
        self.stop_token.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        self.action_id.clone()
    }

    async fn abort_failed(
        mut self,
        agent_files: &NamespaceAgentFiles,
        call_id: &str,
        cmdline: &str,
        approval: &str,
        err: &anyhow::Error,
    ) {
        self.tool_runner.clear_progress_sink(self.pid);
        self.stop_token.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        let mut outcome = serde_json::json!({
            "success": false,
            "error": err.to_string(),
        });
        if let Some(process_error) = err.downcast_ref::<NamespaceToolProcessError>() {
            outcome["process"] = serde_json::json!(format!("/proc/{}", process_error.pid));
            outcome["effect_status"] = serde_json::json!("unknown");
            outcome["reason"] = serde_json::json!(process_error.category.reason());
        }
        let mut envelope = serde_json::json!({
            "call_id": call_id,
            "exit_code": 1,
            "outcome": outcome,
        });
        let _ = crate::runtime::tool_presentation::write_action_metadata(
            &mut envelope,
            "bash",
            &serde_json::json!({"command": cmdline}),
            &outcome,
        );
        let output = redact_durable_evidence_text(
            &serde_json::json!({"stdout": "", "stderr": err.to_string()}).to_string(),
        )
        .text;
        let result = redact_durable_evidence_text(&envelope.to_string()).text;
        let mut record = NamespaceActionRecord::new("bash", "failed")
            .with_output(output)
            .with_result(result)
            .with_approval(approval);
        if let Some(process_error) = err.downcast_ref::<NamespaceToolProcessError>() {
            record = record.with_process(format!("/proc/{}", process_error.pid));
        }
        let mut pending =
            PendingActionPublication::with_retained_id(record, self.action_id.clone());
        if let Err(publish_err) = agent_files.publish_action(&mut pending).await {
            tracing::debug!(error = %publish_err, "failed to publish aborted streaming bash action");
        }
    }
}

fn build_live_bash_result_doc(
    call_id: &str,
    cmdline: &str,
    stdout_tail: &str,
    stderr_tail: &str,
    truncated: bool,
) -> String {
    let redacted_stdout = redact_durable_evidence_text(stdout_tail).text;
    let redacted_stderr = redact_durable_evidence_text(stderr_tail).text;
    let preview_source = if !redacted_stderr.is_empty() && redacted_stdout.is_empty() {
        &redacted_stderr
    } else if !redacted_stderr.is_empty() {
        &format!("{redacted_stdout}\n{redacted_stderr}")
    } else {
        &redacted_stdout
    };
    let preview = preview_source.trim().to_string();
    let presentation = ToolResultPresentation::Command {
        cmdline: cmdline.to_string(),
        exit_code: None,
        stdout: redacted_stdout,
        stderr: redacted_stderr,
        truncated,
    };
    let title = crate::runtime::tool_presentation::tool_title(
        "bash",
        &serde_json::json!({"command": cmdline}),
    )
    .unwrap_or_else(|| "Bash".to_string());
    serde_json::json!({
        "call_id": call_id,
        "title": title,
        "result_preview": preview,
        "presentation": presentation,
    })
    .to_string()
}
