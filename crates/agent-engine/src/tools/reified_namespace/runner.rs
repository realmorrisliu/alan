//! Shared setup/evidence for synchronous probes and cancellable runtime execution.
use super::*;

impl LinuxReifiedNamespaceRunner {
    pub(super) fn run_inner(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let (temp_root, command_spec) = self.prepare(plan)?;
        let mut command = self.captured_command(&temp_root, &command_spec)?;
        let output = match timeout {
            Some(limit) => run_linux_reified_command_with_timeout(command, limit),
            None => command.output(),
        }
        .map_err(|error| self.error(error.to_string(), command_spec.audit_fields()))?;

        self.finish(&temp_root, &command_spec, output)
    }

    pub(super) async fn run_async_inner(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let runner = self.clone();
        let plan = plan.clone();
        // Only trusted setup/probes run on the blocking worker. Dropping this
        // future can never leave a user command running on that worker.
        let (temp_root, command_spec) = tokio::task::spawn_blocking(move || runner.prepare(&plan))
            .await
            .map_err(|error| self.error(format!("runner setup failed: {error}"), Vec::new()))??;
        let output = super::super::sandbox::command_process::output(
            self.captured_command(&temp_root, &command_spec)?.into(),
            timeout,
        )
        .await
        .map_err(|error| self.error(format!("{error:#}"), command_spec.audit_fields()))?;
        self.finish(&temp_root, &command_spec, output)
    }

    fn captured_command(
        &self,
        temp: &ReifiedRunnerTemp,
        spec: &ReifiedNamespaceCommandSpec,
    ) -> Result<Command, ReifiedNamespaceRunError> {
        // Anonymous pipefs handles cannot be bind-mounted as /dev/stdout or
        // /dev/stderr. Private files preserve capture without mounting Host /proc.
        let mut command = spec.command();
        command.stdin(Stdio::null());
        for (name, stdout) in [("stdout", true), ("stderr", false)] {
            let file = std::fs::File::create(temp.parent.join(name)).map_err(|error| {
                self.error(format!("prepare {name}: {error}"), spec.audit_fields())
            })?;
            if stdout {
                command.stdout(file);
            } else {
                command.stderr(file);
            }
        }
        Ok(command)
    }

    fn prepare(
        &self,
        plan: &ReifiedNamespacePlan,
    ) -> Result<(ReifiedRunnerTemp, ReifiedNamespaceCommandSpec), ReifiedNamespaceRunError> {
        if plan.argv.is_empty() {
            return Err(self.error("argv must not be empty", Vec::new()));
        }

        let report = probe_linux_reification();
        let fallback_backend = preferred_linux_backend_with_reification(
            &report,
            matches!(self.fallback_backend, SandboxBackendKind::Landlock),
        );
        if !linux_reification_report_supports_plan(&report, plan.network) {
            return Err(ReifiedNamespaceRunError::new(
                format!(
                    "capability probe did not select reification: {}",
                    linux_reification_unavailable_reasons_for_plan(&report, plan.network)
                        .join("; ")
                ),
                if matches!(fallback_backend, SandboxBackendKind::LinuxReifiedNamespace) {
                    self.fallback_backend
                } else {
                    fallback_backend
                },
                report.audit_fields(),
            ));
        }

        let temp_root = ReifiedRunnerTemp::create(plan)
            .map_err(|err| self.error(format!("create reified root failed: {err}"), Vec::new()))?;
        let command_spec = build_linux_reified_namespace_command(plan, &temp_root)
            .map_err(|err| self.error(err, Vec::new()))?;
        Ok((temp_root, command_spec))
    }

    fn finish(
        &self,
        temp_root: &ReifiedRunnerTemp,
        command_spec: &ReifiedNamespaceCommandSpec,
        output: Output,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let read = |name| {
            std::fs::read(temp_root.parent.join(name)).map_err(|error| {
                self.error(format!("read {name}: {error}"), command_spec.audit_fields())
            })
        };
        let stdout = String::from_utf8_lossy(&read("stdout")?).to_string();
        let stderr = String::from_utf8_lossy(&read("stderr")?).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        if setup_marker_was_written(&temp_root.setup_marker) {
            return Ok(ExecResult {
                stdout,
                stderr,
                exit_code,
            });
        }

        let reason = if stderr.contains(SETUP_FAILURE_PREFIX) {
            stderr.trim().to_string()
        } else {
            format!("namespace setup failed before command execution: exit_code={exit_code}")
        };
        Err(self.error(reason, command_spec.audit_fields()))
    }

    fn error(
        &self,
        reason: impl Into<String>,
        mut audit_fields: Vec<(&'static str, String)>,
    ) -> ReifiedNamespaceRunError {
        audit_fields.extend([
            (
                "backend",
                SandboxBackendKind::LinuxReifiedNamespace.name().to_string(),
            ),
            ("status", "unavailable".to_string()),
            ("fallback_backend", self.fallback_backend.name().to_string()),
        ]);
        ReifiedNamespaceRunError::new(reason, self.fallback_backend, audit_fields)
    }
}
