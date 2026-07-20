//! Process lifecycle and stream files exposed through `/proc`.

use std::collections::BTreeMap;

use alan_agent_protocol::{
    AgentExecutableRequest, AgentExecutableResult, ProcessExecSpec, ProcessNamespaceAccess,
    ProcessNamespaceManifest, ProcessNamespaceMount,
};
use anyhow::{Context, Result, bail};

use super::{NamespaceProcessFiles, client::NamespaceClient};

impl NamespaceProcessFiles {
    fn client(&self) -> NamespaceClient {
        NamespaceClient::new(self.root.clone())
    }

    /// Authoritative `/proc/<pid>` path corresponding to this AgentFS view.
    pub fn process_path(&self) -> Result<String> {
        Ok(format!("/proc/{}", agent_pid_from_path(&self.agent_path)?))
    }

    pub(crate) fn current_pid(&self) -> Result<&str> {
        agent_pid_from_path(&self.agent_path)
    }

    pub(crate) async fn read_process_namespace(
        &self,
        pid: &str,
    ) -> Result<Vec<ProcessNamespaceMount>> {
        let path = format!("/proc/{pid}/namespace");
        let document = String::from_utf8(
            self.client()
                .read_file(&path)
                .await
                .with_context(|| format!("read Process namespace from {path}"))?,
        )
        .context("Process namespace is utf8")?;
        document
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (path, access) = line
                    .rsplit_once(' ')
                    .with_context(|| format!("invalid Process namespace entry `{line}`"))?;
                let access = match access {
                    "ro" => ProcessNamespaceAccess::ReadOnly,
                    "rw" => ProcessNamespaceAccess::ReadWrite,
                    other => bail!("invalid Process namespace access `{other}`"),
                };
                Ok(ProcessNamespaceMount::new(path, access))
            })
            .collect()
    }

    pub(crate) async fn read_process_descriptors(
        &self,
        pid: &str,
    ) -> Result<BTreeMap<u32, String>> {
        let path = format!("/proc/{pid}/descriptors");
        serde_json::from_slice(
            &self
                .client()
                .read_file(&path)
                .await
                .with_context(|| format!("read Process descriptors from {path}"))?,
        )
        .with_context(|| format!("parse Process descriptors from {path}"))
    }

    pub async fn write_process_control_for_pid(&self, pid: &str, command: &str) -> Result<()> {
        let ctl_path = format!("/proc/{pid}/ctl");
        self.client()
            .write_document(&ctl_path, command.as_bytes())
            .await
            .with_context(|| format!("write process control command to {ctl_path}"))
    }

    /// Read terminal process state from authoritative `/proc`.
    pub(crate) async fn read_process_exit_code(&self, pid: &str) -> Result<Option<i32>> {
        let client = self.client();
        let status_path = format!("/proc/{pid}/status");
        let status = String::from_utf8(
            client
                .read_file(&status_path)
                .await
                .with_context(|| format!("read process status from {status_path}"))?,
        )
        .context("process status is utf8")?;
        if status.trim() != "exited" {
            return Ok(None);
        }
        let exit_path = format!("/proc/{pid}/exit");
        let exit = String::from_utf8(
            client
                .read_file(&exit_path)
                .await
                .with_context(|| format!("read process exit from {exit_path}"))?,
        )
        .context("process exit is utf8")?;
        let code = exit
            .trim()
            .parse::<i32>()
            .with_context(|| format!("parse process exit code from {exit_path}"))?;
        Ok(Some(code))
    }

    pub(crate) async fn read_process_io_offsets(&self, pid: &str) -> Result<(u64, u64)> {
        let client = self.client();
        let output_path = format!("/proc/{pid}/io/output");
        let events_path = format!("/proc/{pid}/io/events");
        let output = client
            .stat_path(&output_path)
            .await
            .with_context(|| format!("stat process output at {output_path}"))?
            .length;
        let events = client
            .stat_path(&events_path)
            .await
            .with_context(|| format!("stat process IO events at {events_path}"))?
            .length;
        Ok((output, events))
    }

    pub(crate) async fn read_agent_process_result(
        &self,
        pid: &str,
    ) -> Result<AgentExecutableResult> {
        AgentExecutableResult::from_process_output(&self.read_process_output(pid).await?)
            .with_context(|| format!("parse Agent Executable result from /proc/{pid}/io/output"))
    }

    pub(crate) async fn read_process_output(&self, pid: &str) -> Result<Vec<u8>> {
        let output_path = format!("/proc/{pid}/io/output");
        self.client()
            .read_file(&output_path)
            .await
            .with_context(|| format!("read Process output from {output_path}"))
    }

    pub async fn spawn_process<I, S>(
        &self,
        executable: &str,
        args: I,
        mounts: Vec<ProcessNamespaceMount>,
    ) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let exec_spec = serde_json::to_vec(&ProcessExecSpec {
            executable: executable.to_string(),
            args,
            namespace: ProcessNamespaceManifest { mounts },
            descriptors: BTreeMap::new(),
        })
        .context("serialize exec spec")?;
        self.client()
            .clone_with_document("/proc/clone", &exec_spec)
            .await
            .with_context(|| format!("spawn {executable} through /proc/clone"))
    }

    pub(crate) async fn spawn_agent_process(
        &self,
        request: &AgentExecutableRequest,
        mounts: Vec<ProcessNamespaceMount>,
        descriptors: BTreeMap<u32, String>,
    ) -> Result<String> {
        let request =
            serde_json::to_string(request).context("serialize /bin/alan-agent launch request")?;
        let exec_spec = serde_json::to_vec(&ProcessExecSpec {
            executable: "/bin/alan-agent".to_string(),
            args: vec![request],
            namespace: ProcessNamespaceManifest { mounts },
            descriptors,
        })
        .context("serialize /bin/alan-agent exec spec")?;
        self.client()
            .clone_with_document("/proc/clone", &exec_spec)
            .await
            .context("spawn /bin/alan-agent through /proc/clone")
    }

    pub(super) async fn read_process_result(
        &self,
        pid: &str,
        timeout_secs: usize,
    ) -> Result<NamespaceProcessResult> {
        if timeout_secs == 0 {
            return self.read_process_result_until_exit(pid).await;
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs as u64),
            self.read_process_result_until_exit(pid),
        )
        .await
        .with_context(|| format!("timed out waiting {timeout_secs}s for process {pid} to exit"))?
    }

    async fn read_process_result_until_exit(&self, pid: &str) -> Result<NamespaceProcessResult> {
        let client = self.client();
        let status_path = format!("/proc/{pid}/status");
        let exit_path = format!("/proc/{pid}/exit");
        let output_path = format!("/proc/{pid}/io/output");
        loop {
            let status = String::from_utf8(
                client
                    .read_file(&status_path)
                    .await
                    .with_context(|| format!("read {status_path}"))?,
            )
            .context("process status is not utf8")?;
            if status.trim() == "exited" {
                let exit_code = String::from_utf8(
                    client
                        .read_file(&exit_path)
                        .await
                        .with_context(|| format!("read {exit_path}"))?,
                )
                .context("process exit code is not utf8")?
                .trim()
                .parse::<i32>()
                .context("process exit code is not an integer")?;
                let output = if client
                    .stat_path(&output_path)
                    .await
                    .with_context(|| format!("stat {output_path}"))?
                    .length
                    == 0
                {
                    String::new()
                } else {
                    String::from_utf8(
                        client
                            .read_file(&output_path)
                            .await
                            .with_context(|| format!("read {output_path}"))?,
                    )
                    .context("process output is not utf8")?
                };
                return Ok(NamespaceProcessResult { output, exit_code });
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

pub(super) struct NamespaceProcessResult {
    pub(super) output: String,
    pub(super) exit_code: i32,
}

fn agent_pid_from_path(agent_path: &str) -> Result<&str> {
    let components = agent_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    match components.as_slice() {
        ["agent", pid] if *pid != "root" => Ok(*pid),
        ["agent", "root"] => {
            bail!("process control requires a concrete /agent/<pid> path, got /agent/root")
        }
        _ => bail!("invalid agent path for process control: {agent_path}"),
    }
}
