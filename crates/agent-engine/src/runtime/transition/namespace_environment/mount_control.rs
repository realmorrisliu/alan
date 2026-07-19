//! Approved Namespace mount mutation and dependent Tool binding projection.

use super::{ApprovedMountGrant, NamespaceMountApplication, NamespaceMountControl};

impl NamespaceMountControl<'_> {
    /// Retain logical projection metadata after Host Mount Service commits the live mount.
    pub fn record_projected_host_mount(
        &mut self,
        namespace_path: String,
        access: alan_kernel::Access,
    ) {
        if let Some(context) = self.launch_context.as_mut() {
            context.record_projected_host_mount(namespace_path, access);
        }
    }

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
}
