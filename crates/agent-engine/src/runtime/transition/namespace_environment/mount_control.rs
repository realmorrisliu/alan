//! Approved Namespace mount mutation and dependent Tool binding projection.

use std::path::PathBuf;

use super::{ApprovedMountGrant, NamespaceMountApplication, NamespaceMountControl};

impl NamespaceMountControl<'_> {
    /// Apply an approved grant to the live Namespace and retain its new snapshot.
    pub fn apply_approved_grant(
        &mut self,
        grant: &ApprovedMountGrant,
    ) -> NamespaceMountApplication {
        let Some(applicator) = self.mount_grant_applicator.clone() else {
            return NamespaceMountApplication::unavailable(
                "live namespace mount applicator unavailable",
            );
        };
        match applicator.apply_mount_grant(grant) {
            Ok(namespace) => {
                if let Some(context) = self.launch_context.as_mut() {
                    context.namespace = namespace;
                }
                NamespaceMountApplication::applied()
            }
            Err(error) => NamespaceMountApplication::failed(error),
        }
    }

    pub(crate) fn retain_host_mount(&mut self, grant: crate::HostMountGrant) -> bool {
        let Some(context) = self.launch_context.as_mut() else {
            return false;
        };
        if let Some(index) = context
            .host_mounts
            .iter()
            .position(|existing| existing.namespace_path == grant.namespace_path)
        {
            let changed = context.host_mounts[index] != grant;
            context.host_mounts[index] = grant;
            return changed;
        }
        context.host_mounts.push(grant);
        true
    }

    pub(crate) fn sync_tool_binding(&self, scratch_dir: PathBuf) -> bool {
        let Some(launch_context) = self.launch_context.as_ref() else {
            return false;
        };
        let Ok(binding) =
            crate::tools::ToolExecutionBinding::from_launch_context(launch_context, scratch_dir)
        else {
            return false;
        };
        let Some(process_context) = self.tool_process_context.as_ref() else {
            return false;
        };
        let changed = process_context
            .tool_runner
            .process_binding(process_context.pid)
            != Some(binding.clone());
        process_context
            .tool_runner
            .register_process_binding(process_context.pid, binding);
        changed
    }
}
