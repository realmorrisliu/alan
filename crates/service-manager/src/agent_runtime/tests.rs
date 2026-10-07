use std::sync::Arc;

use alan_ap::InProcessTransport;
use alan_kernel::{Access, LiveNamespace, Namespace};

use super::{resolve_child_connection, validate_child_memory_mount, validate_process_cwd};

#[test]
fn process_cwd_must_be_reachable_after_service_projection() {
    let namespace = LiveNamespace::new(Namespace::new());
    assert!(validate_process_cwd(&namespace, "/").is_ok());
    assert!(validate_process_cwd(&namespace, "/mnt/review").is_err());

    namespace.mount(
        "/mnt/review",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    assert!(validate_process_cwd(&namespace, "/mnt/review/src").is_ok());
}

#[test]
fn child_connection_must_be_passed_by_the_parent() {
    assert_eq!(
        resolve_child_connection("parent-profile", None).unwrap(),
        "parent-profile"
    );
    assert_eq!(
        resolve_child_connection("parent-profile", Some("parent-profile")).unwrap(),
        "parent-profile"
    );
    let error = resolve_child_connection("parent-profile", Some("other-profile")).unwrap_err();
    assert!(error.to_string().contains("other-profile"));
    assert!(error.to_string().contains("parent-profile"));
}

#[test]
fn child_memory_mount_requires_the_memory_handle() {
    for mount in ["/memory", "/memory/root", "/memory/root/session"] {
        let mut namespace = Namespace::new();
        namespace.mount(
            mount,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadWrite,
        );

        assert!(validate_child_memory_mount(&namespace, true, Some("/memory/root")).is_ok());
        let error =
            validate_child_memory_mount(&namespace, false, Some("/memory/root")).unwrap_err();
        assert!(error.to_string().contains("without the Memory handle"));
    }
}
