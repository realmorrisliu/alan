//! Process-hosted Tool execution owner: materializes Tool packages and runs
//! Tool executables inside one Process namespace.
//!
//! Extracted from `registry.rs` to keep the registry owner small; it remains a
//! sibling of `registry` and accesses registry state through the parent module.

use super::{Tool, ToolRegistry};
use crate::config::Config;
use crate::tools::{ToolContext, ToolExecutionBinding};
use anyhow::{Context, Result};
use jsonschema::Draft;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Process-server host for Tool executables reached through `/proc/clone`.
#[derive(Clone)]
pub struct ToolProcessRunner {
    inner: Arc<ToolProcessRunnerInner>,
}

type ProcessToolsMap = HashMap<u64, HashMap<String, Arc<dyn Tool>>>;

struct ToolProcessRunnerInner {
    tools: HashMap<String, Arc<dyn Tool>>,
    config: Arc<Config>,
    default_binding: Arc<Mutex<Option<ToolExecutionBinding>>>,
    process_bindings: Mutex<HashMap<u64, ToolExecutionBinding>>,
    process_authorities: Mutex<HashMap<u64, Arc<dyn crate::tools::ToolExecutionAuthority>>>,
    process_trust_postures: Mutex<HashMap<u64, crate::policy::TrustPosture>>,
    process_tools: Mutex<ProcessToolsMap>,
    read_only_receipts: Mutex<BTreeMap<u64, (u64, String, String)>>,
    progress_sinks:
        Mutex<HashMap<u64, crate::tools::sandbox::command_process::OutputProgressCallback>>,
}

/// Kernel-neutral inputs for one Tool executable invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProcessInvocation {
    pub pid: u64,
    pub parent: Option<u64>,
    pub executable: String,
    pub args: Vec<String>,
}

/// Kernel-neutral terminal result produced by one Tool executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProcessOutcome {
    pub output: Vec<u8>,
    pub exit_code: i32,
}

impl ToolProcessOutcome {
    pub fn exited(exit_code: i32, output: impl Into<Vec<u8>>) -> Self {
        Self {
            output: output.into(),
            exit_code,
        }
    }
}

impl ToolProcessRunner {
    pub(crate) fn from_registry(registry: &ToolRegistry) -> Self {
        Self {
            inner: Arc::new(ToolProcessRunnerInner {
                tools: registry.tools.clone(),
                config: Arc::clone(&registry.config),
                default_binding: Arc::clone(&registry.default_binding),
                process_bindings: Mutex::new(HashMap::new()),
                process_authorities: Mutex::new(HashMap::new()),
                process_trust_postures: Mutex::new(HashMap::new()),
                process_tools: Mutex::new(HashMap::new()),
                read_only_receipts: Mutex::new(BTreeMap::new()),
                progress_sinks: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Register an incremental command output sink for the active Tool Process owner.
    pub(crate) fn set_progress_sink(
        &self,
        pid: u64,
        sink: crate::tools::sandbox::command_process::OutputProgressCallback,
    ) {
        self.inner
            .progress_sinks
            .lock()
            .expect("progress sink mutex poisoned")
            .insert(pid, sink);
    }

    /// Remove any incremental command output sink registered for `pid`.
    pub(crate) fn clear_progress_sink(&self, pid: u64) {
        self.inner
            .progress_sinks
            .lock()
            .expect("progress sink mutex poisoned")
            .remove(&pid);
    }

    /// Bind host-resolved Tool execution authority to one assembled Process.
    pub fn register_process_binding(&self, pid: u64, binding: ToolExecutionBinding) {
        self.inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .insert(pid, binding);
    }

    pub(crate) fn change_process_directory(&self, pid: u64, path: &Path) -> Result<PathBuf> {
        let mut binding = self
            .inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .get(&pid)
            .cloned()
            .context("Process has no Tool execution binding")?;
        let authority = self
            .inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .get(&pid)
            .cloned()
            .context("Process has no live Host Mount authority")?;
        // An explicit absolute cd may select a replacement grant after revocation.
        if path.is_absolute() {
            binding.namespace_cwd = path.to_path_buf();
            binding.cwd_grant_id = None;
        }
        let previous_namespace_cwd = binding.namespace_cwd.clone();
        let had_host_adapter = binding.has_adapter();
        binding = authority.reconcile(pid, binding)?;
        anyhow::ensure!(
            !had_host_adapter
                || binding.namespace_cwd == previous_namespace_cwd
                || path.is_absolute(),
            "Process cwd is no longer authorized; choose an explicit directory"
        );
        let adapter = binding
            .adapter()
            .context("Process has no active Host Mount execution adapter")?;
        let namespace_cwd = adapter.resolve_directory(&binding.namespace_cwd, path)?;
        binding.namespace_cwd = namespace_cwd.clone();
        binding = authority.reconcile(pid, binding)?;
        anyhow::ensure!(
            binding.namespace_cwd == namespace_cwd,
            "Host Mount authority changed the selected cwd during validation"
        );
        self.register_process_binding(pid, binding);
        Ok(namespace_cwd)
    }

    pub(crate) fn process_binding(&self, pid: u64) -> Option<ToolExecutionBinding> {
        self.inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .get(&pid)
            .cloned()
    }

    pub(crate) fn read_only_presentation_scope(
        &self,
        pid: u64,
        name: &str,
        arguments: &Value,
    ) -> Option<String> {
        let tool = self.inner.tools.get(name)?;
        if !tool.presentation_is_read_only()
            || tool.capability(arguments) != alan_agent_protocol::ToolCapability::Read
        {
            return None;
        }
        self.process_presentation_scope(pid)
    }

    fn process_presentation_scope(&self, pid: u64) -> Option<String> {
        let binding = self.process_binding(pid)?;
        if !binding.has_adapter() {
            return None;
        }
        let authority = self
            .inner
            .process_authorities
            .lock()
            .ok()?
            .get(&pid)?
            .clone();
        let scope = authority.presentation_scope(pid)?;
        let cwd = binding.namespace_cwd.to_str()?;
        if scope.is_empty() || scope.len() > 4096 || cwd.len() > 4096 {
            return None;
        }
        let bytes = serde_json::to_vec(&(cwd, scope)).ok()?;
        Some(hex::encode(Sha256::digest(bytes)))
    }

    pub(crate) fn take_read_only_receipt(
        &self,
        pid: u64,
        parent: u64,
        name: &str,
    ) -> Option<String> {
        let (owner, tool, scope) = self.inner.read_only_receipts.lock().ok()?.remove(&pid)?;
        (owner == parent && tool == name).then_some(scope)
    }

    /// Install a late-bound authority resolver for one Agent Process.
    pub fn register_process_authority(
        &self,
        pid: u64,
        authority: Arc<dyn crate::tools::ToolExecutionAuthority>,
    ) {
        self.inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .insert(pid, authority);
    }

    /// Record the explicit invocation trust posture governing one Agent Process.
    pub fn register_process_trust_posture(
        &self,
        pid: u64,
        trust_posture: crate::policy::TrustPosture,
    ) {
        self.inner
            .process_trust_postures
            .lock()
            .expect("process trust posture mutex poisoned")
            .insert(pid, trust_posture);
    }

    /// Return the explicit invocation trust posture registered for one Agent Process, if any.
    pub fn process_trust_posture(&self, pid: u64) -> Option<crate::policy::TrustPosture> {
        self.inner
            .process_trust_postures
            .lock()
            .expect("process trust posture mutex poisoned")
            .get(&pid)
            .copied()
    }

    /// Remove all Tool execution state when its owning Process exits.
    pub fn unregister_process(&self, pid: u64) {
        let inner = &self.inner;
        inner
            .process_bindings
            .lock()
            .expect("binding mutex poisoned")
            .remove(&pid);
        inner
            .process_authorities
            .lock()
            .expect("authority mutex poisoned")
            .remove(&pid);
        inner
            .process_trust_postures
            .lock()
            .expect("process trust posture mutex poisoned")
            .remove(&pid);
        inner
            .process_tools
            .lock()
            .expect("process tools mutex poisoned")
            .remove(&pid);
    }

    /// Register Process-scoped packaged or projected MCP Tools for `pid`.
    /// Built-in core Tools cannot be shadowed by Process-scoped packages.
    pub fn register_process_tools(&self, pid: u64, tools: Vec<Arc<dyn Tool>>) -> Result<()> {
        let mut per_process = HashMap::new();
        for tool in tools {
            anyhow::ensure!(
                !self.inner.tools.contains_key(tool.name()),
                "packaged Tool `{}` cannot shadow a built-in Alan Tool",
                tool.name()
            );
            anyhow::ensure!(
                per_process.insert(tool.name().to_string(), tool).is_none(),
                "duplicate packaged Tool name projected for Process {pid}"
            );
        }
        self.inner
            .process_tools
            .lock()
            .expect("process tools mutex poisoned")
            .insert(pid, per_process);
        Ok(())
    }

    pub(crate) fn capability_for_tool(
        &self,
        name: &str,
        arguments: &Value,
    ) -> Option<alan_agent_protocol::ToolCapability> {
        self.inner
            .tools
            .get(name)
            .map(|tool| tool.capability(arguments))
    }

    /// Whether the named Tool declares this exact invocation safe to repeat.
    pub(crate) fn allows_idempotent_retry_for_tool(&self, name: &str, arguments: &Value) -> bool {
        self.inner
            .tools
            .get(name)
            .is_some_and(|tool| tool.allows_idempotent_retry(arguments))
    }

    pub(crate) fn authority_revision(&self, pid: u64) -> Option<u64> {
        let authority = self
            .inner
            .process_authorities
            .lock()
            .ok()?
            .get(&pid)?
            .clone();
        authority.authority_revision(pid)
    }

    pub(crate) fn git_push_delegations(
        &self,
        pid: u64,
    ) -> Vec<crate::policy::GitPushDelegationScope> {
        let Some(authority) = self
            .inner
            .process_authorities
            .lock()
            .ok()
            .and_then(|authorities| authorities.get(&pid).cloned())
        else {
            return Vec::new();
        };
        authority.git_push_delegations(pid)
    }

    pub(crate) fn project_knowledge_roots(&self, pid: u64) -> Vec<PathBuf> {
        let Some(authority) = self
            .inner
            .process_authorities
            .lock()
            .ok()
            .and_then(|authorities| authorities.get(&pid).cloned())
        else {
            return Vec::new();
        };
        authority.project_knowledge_roots(pid)
    }
}

impl ToolProcessRunner {
    /// Execute one Tool image after the Alan OS adapter has validated its namespace binding.
    pub async fn run(&self, invocation: ToolProcessInvocation) -> ToolProcessOutcome {
        self.run_with_namespace(invocation, None).await
    }

    /// Execute with the actual Process namespace supplied by its owning OS service.
    pub async fn run_in_namespace(
        &self,
        invocation: ToolProcessInvocation,
        namespace: alan_ap::InProcessTransport,
    ) -> ToolProcessOutcome {
        self.run_with_namespace(invocation, Some(namespace)).await
    }

    async fn run_with_namespace(
        &self,
        invocation: ToolProcessInvocation,
        namespace: Option<alan_ap::InProcessTransport>,
    ) -> ToolProcessOutcome {
        let name = invocation
            .executable
            .rsplit('/')
            .next()
            .unwrap_or(invocation.executable.as_str());
        let authority_pid = invocation.parent.unwrap_or(invocation.pid);
        let packaged_tool = self
            .inner
            .process_tools
            .lock()
            .expect("process tools mutex poisoned")
            .get(&authority_pid)
            .and_then(|map| map.get(name))
            .cloned();
        let Some(tool) = self.inner.tools.get(name).cloned().or(packaged_tool) else {
            return ToolProcessOutcome::exited(127, b"Tool executable has no host\n");
        };
        let arguments = invocation
            .args
            .first()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(Value::Null);
        let schema = tool.parameters_schema();
        let validator = match jsonschema::options()
            .with_draft(Draft::Draft7)
            .build(&schema)
        {
            Ok(validator) => validator,
            Err(error) => {
                return process_json_outcome(
                    1,
                    serde_json::json!({"success": false, "error": format!("invalid Tool schema: {error}")}),
                );
            }
        };
        let errors = validator
            .iter_errors(&arguments)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": errors.join("; ")}),
            );
        }
        let process_binding = self
            .inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .get(&invocation.parent.unwrap_or(invocation.pid))
            .cloned();
        let binding = process_binding.or_else(|| {
            self.inner
                .default_binding
                .lock()
                .expect("default binding mutex poisoned")
                .clone()
        });
        let Some(mut binding) = binding else {
            return process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": "Tool Process has no explicit execution binding"}),
            );
        };
        let authority_pid = invocation.parent.unwrap_or(invocation.pid);
        let authority = self
            .inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .get(&authority_pid)
            .cloned();
        if let Some(authority) = authority {
            binding = match authority.reconcile(authority_pid, binding) {
                Ok(binding) => binding,
                Err(error) => {
                    return process_json_outcome(
                        1,
                        serde_json::json!({
                            "success": false,
                            "error": format!("Tool Process authority is unavailable: {error}"),
                        }),
                    );
                }
            };
            self.register_process_binding(authority_pid, binding.clone());
        }
        let trust_posture = self
            .inner
            .process_trust_postures
            .lock()
            .expect("process trust posture mutex poisoned")
            .get(&authority_pid)
            .copied()
            .unwrap_or_default();
        let mut context = ToolContext::from_binding(binding, Arc::clone(&self.inner.config))
            .with_trust_posture(trust_posture);
        if let Some(namespace) = namespace {
            context = context.with_namespace(authority_pid, namespace);
        }
        if name == "bash"
            && let Some(sink) = self
                .inner
                .progress_sinks
                .lock()
                .expect("progress sink mutex poisoned")
                .get(&authority_pid)
                .cloned()
        {
            context = context.with_progress(sink);
        }
        let timeout_secs = if self.inner.config.tool_timeout_secs != 30 {
            self.inner.config.tool_timeout_secs
        } else {
            tool.timeout_secs()
        };
        let read_only_before = self.read_only_presentation_scope(authority_pid, name, &arguments);
        let execution = tool.execute(arguments, &context);
        let result = if timeout_secs == 0 {
            execution.await
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs as u64),
                execution,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!(
                    "Tool execution timed out after {timeout_secs}s"
                )),
            }
        };
        match result {
            Ok(mut output) => {
                context.project_value(&mut output);
                if let Some(scope) = read_only_before
                    && self.process_presentation_scope(authority_pid).as_ref() == Some(&scope)
                {
                    let mut receipts = self
                        .inner
                        .read_only_receipts
                        .lock()
                        .expect("receipt mutex poisoned");
                    receipts.insert(invocation.pid, (authority_pid, name.into(), scope));
                    // shortcut: retain 128 late presentation receipts; expand if routine bursts lose grouping.
                    while receipts.len() > 128 {
                        receipts.pop_first();
                    }
                }
                process_json_outcome(0, output)
            }
            Err(error) => process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": context.project_text(&format!("{error:#}"))}),
            ),
        }
    }
}

fn process_json_outcome(exit_code: i32, value: Value) -> ToolProcessOutcome {
    let mut bytes = serde_json::to_vec(&value).unwrap_or_else(|_| {
        serde_json::to_vec(&serde_json::json!({"success": exit_code == 0})).unwrap()
    });
    bytes.push(b'\n');
    ToolProcessOutcome::exited(exit_code, bytes)
}

#[cfg(test)]
#[async_trait::async_trait]
impl alan_kernel::ProcessRunner for ToolProcessRunner {
    async fn run(&self, invocation: alan_kernel::ProcessInvocation) -> alan_kernel::ProcessOutcome {
        if invocation
            .namespace
            .resolve(&invocation.exec.executable)
            .is_err()
        {
            return alan_kernel::ProcessOutcome::exited(127, b"executable is not mounted\n");
        }
        let mount_fs = alan_kernel::MountFs::new(invocation.namespace)
            .with_read_only_prefixes(crate::tools::TOOL_READ_ONLY_SYSTEM_PREFIXES);
        let outcome = self
            .run_in_namespace(
                ToolProcessInvocation {
                    pid: invocation.pid.0,
                    parent: invocation.parent.map(|pid| pid.0),
                    executable: invocation.exec.executable,
                    args: invocation.exec.args,
                },
                alan_ap::InProcessTransport::new(Arc::new(mount_fs)),
            )
            .await;
        alan_kernel::ProcessOutcome::exited(outcome.exit_code, outcome.output)
    }
}
