//! Approved Namespace mount mutation and dependent Tool binding projection.

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
}
