//! Host-side `ServiceActivationAuthority` implementation for the primary Root Agent Process.

use std::sync::Arc;

use alan_agent_engine::TrustPosture;
use alan_agent_engine::runtime::service_activation::{
    ActivatedServiceMount, AvailableFileserverPackage, ServiceActivationAuthority,
    ServiceActivationRequest, ServiceMountAccess,
};
use alan_kernel::{Access, LiveNamespace, SrvFs};
use async_trait::async_trait;

use crate::fileserver_service::FileserverManager;
use crate::local_entry::LocalEntryService;
use crate::runtime::embed_mounts::RESERVED_MNT_ROOTS;
use crate::runtime::packages::prepare_fileserver_launch_grant;
use crate::{
    FileserverMountAccess, PackageExportKind, PackageService, PackageState, RootBindingMode,
    RootFileserverActivation, validate_package_id,
};

#[derive(Clone)]
pub(crate) struct ServiceActivationDeps {
    pub(crate) packages: Arc<PackageService>,
    pub(crate) fileserver_manager: Arc<FileserverManager>,
    pub(crate) srvfs: Arc<SrvFs>,
    pub(crate) system_namespace: LiveNamespace,
    pub(crate) local_entry: Arc<LocalEntryService>,
}

impl ServiceActivationDeps {
    pub(crate) fn for_root(
        &self,
        root_namespace: LiveNamespace,
        trust_posture: TrustPosture,
    ) -> Arc<dyn ServiceActivationAuthority> {
        Arc::new(ProcessServiceActivation {
            packages: Arc::clone(&self.packages),
            fileserver_manager: Arc::clone(&self.fileserver_manager),
            srvfs: Arc::clone(&self.srvfs),
            system_namespace: self.system_namespace.clone(),
            local_entry: Arc::clone(&self.local_entry),
            root_namespace,
            trust_posture,
        })
    }
}

/// Root-only authority that inspects, starts/upgrades, and hot-mounts `fileserver` packages.
pub(crate) struct ProcessServiceActivation {
    packages: Arc<PackageService>,
    fileserver_manager: Arc<FileserverManager>,
    srvfs: Arc<SrvFs>,
    system_namespace: LiveNamespace,
    local_entry: Arc<LocalEntryService>,
    root_namespace: LiveNamespace,
    trust_posture: TrustPosture,
}

impl ProcessServiceActivation {
    fn active_grant_for_package(
        &self,
        package_id: &str,
        service_id: &str,
    ) -> Option<(ServiceMountAccess, bool)> {
        let mount_path = format!("/mnt/{service_id}");
        let status = self.fileserver_manager.status_snapshot(service_id)?;
        if !status.running || status.package_id != package_id {
            return None;
        }
        self.root_namespace.snapshot().resolve(&mount_path).ok()?;
        Some((
            kernel_to_service_access(status.mount_access),
            status.allow_network,
        ))
    }
}

#[async_trait]
impl ServiceActivationAuthority for ProcessServiceActivation {
    fn list_available_fileservers(&self) -> Vec<AvailableFileserverPackage> {
        let Ok(catalog) = self.packages.catalog() else {
            return Vec::new();
        };
        let mut out = catalog
            .packages
            .into_values()
            .filter(|record| {
                record.state == PackageState::Installed
                    && record.export_kind == PackageExportKind::Fileserver
            })
            .filter_map(|record| {
                let export = record.fileserver_export?;
                if RESERVED_MNT_ROOTS.contains(&export.service_id.as_str()) {
                    return None;
                }
                let mount_path = format!("/mnt/{}", export.service_id);
                let active_grant = self.active_grant_for_package(&record.id, &export.service_id);
                Some(AvailableFileserverPackage {
                    package_id: record.id,
                    service_id: export.service_id,
                    mount_path,
                    description: export.description.or(record.description),
                    allow_network: export.allow_network,
                    network_domains: export.network_domains,
                    active: active_grant.is_some(),
                    active_access: active_grant.map(|(access, _)| access),
                    active_allow_network: active_grant
                        .is_some_and(|(_, allow_network)| allow_network),
                })
            })
            .collect::<Vec<_>>();
        out.sort_by(|left, right| left.package_id.cmp(&right.package_id));
        out
    }

    fn inspect_request(
        &self,
        request: &ServiceActivationRequest,
    ) -> Result<AvailableFileserverPackage, String> {
        let pkg_id = request.package_id.trim();
        validate_package_id(pkg_id).map_err(|err| err.to_string())?;
        if request.reason.trim().is_empty() {
            return Err("reason must be a non-empty string".to_string());
        }
        let record = self
            .packages
            .resolve(pkg_id)
            .map_err(|err| err.to_string())?;
        if record.state != PackageState::Installed {
            return Err(format!("package `{pkg_id}` is not installed"));
        }
        if record.export_kind != PackageExportKind::Fileserver {
            return Err(format!(
                "package `{pkg_id}` is `{}` rather than `fileserver`",
                record.export_kind.as_str()
            ));
        }
        let export = record
            .fileserver_export
            .as_ref()
            .ok_or_else(|| format!("fileserver package `{pkg_id}` lacks a fileserver export"))?;
        if RESERVED_MNT_ROOTS.contains(&export.service_id.as_str()) {
            return Err(format!("service `{}` is reserved", export.service_id));
        }
        if request.allow_network && !export.allow_network {
            return Err(format!(
                "fileserver package `{pkg_id}` does not allow network access"
            ));
        }
        if let Some(existing) = self.fileserver_manager.status_snapshot(&export.service_id)
            && existing.running
            && existing.package_id != record.id
        {
            return Err(format!(
                "service `{}` at `/mnt/{}` is already occupied by package `{}`",
                export.service_id, export.service_id, existing.package_id
            ));
        }
        let mount_path = format!("/mnt/{}", export.service_id);
        let active_grant = self.active_grant_for_package(&record.id, &export.service_id);
        Ok(AvailableFileserverPackage {
            package_id: record.id,
            service_id: export.service_id.clone(),
            mount_path,
            description: export.description.clone().or(record.description),
            allow_network: export.allow_network,
            network_domains: export.network_domains.clone(),
            active: active_grant.is_some(),
            active_access: active_grant.map(|(access, _)| access),
            active_allow_network: active_grant.is_some_and(|(_, allow_network)| allow_network),
        })
    }

    fn active_service_grant(&self, service_id: &str) -> Option<(ServiceMountAccess, bool)> {
        let status = self.fileserver_manager.status_snapshot(service_id)?;
        if !status.running {
            return None;
        }
        self.root_namespace
            .snapshot()
            .resolve(&status.mount_path)
            .ok()?;
        Some((
            kernel_to_service_access(status.mount_access),
            status.allow_network,
        ))
    }

    async fn activate_fileserver(
        &self,
        request: &ServiceActivationRequest,
        persist_for_root: bool,
    ) -> Result<ActivatedServiceMount, String> {
        let available = self.inspect_request(request)?;
        let mut started_fresh = false;

        let status = match self.fileserver_manager.status(&available.service_id).await {
            Some(existing) if existing.running => {
                if existing.package_id != available.package_id {
                    return Err(format!(
                        "service `{}` is already active for package `{}`",
                        available.service_id, existing.package_id
                    ));
                }
                let existing_access = kernel_to_service_access(existing.mount_access);
                let sufficient = existing_access.satisfies(request.access)
                    && (!request.allow_network || existing.allow_network);
                if sufficient {
                    existing
                } else {
                    let target_access = if existing.mount_access == Access::ReadWrite
                        || request.access == ServiceMountAccess::ReadWrite
                    {
                        FileserverMountAccess::ReadWrite
                    } else {
                        FileserverMountAccess::ReadOnly
                    };
                    let target_network = existing.allow_network || request.allow_network;
                    self.fileserver_manager
                        .stop(&available.service_id)
                        .await
                        .map_err(|err| err.to_string())?;
                    started_fresh = true;
                    let grant = prepare_fileserver_launch_grant(
                        &available.package_id,
                        &available.service_id,
                        target_access,
                        target_network,
                        self.trust_posture,
                        None,
                        None,
                    );
                    self.fileserver_manager
                        .start(grant)
                        .await
                        .map_err(|err| err.to_string())?
                }
            }
            Some(_) => {
                let _ = self.fileserver_manager.stop(&available.service_id).await;
                started_fresh = true;
                let grant = prepare_fileserver_launch_grant(
                    &available.package_id,
                    &available.service_id,
                    service_to_fileserver_access(request.access),
                    request.allow_network,
                    self.trust_posture,
                    None,
                    None,
                );
                self.fileserver_manager
                    .start(grant)
                    .await
                    .map_err(|err| err.to_string())?
            }
            None => {
                started_fresh = true;
                let grant = prepare_fileserver_launch_grant(
                    &available.package_id,
                    &available.service_id,
                    service_to_fileserver_access(request.access),
                    request.allow_network,
                    self.trust_posture,
                    None,
                    None,
                );
                self.fileserver_manager
                    .start(grant)
                    .await
                    .map_err(|err| err.to_string())?
            }
        };

        let mount_result = async {
            let (transport, posted_access) = self
                .srvfs
                .lookup(&status.service_id)
                .await
                .ok_or_else(|| format!("/srv/{} is not published", status.service_id))?;
            let effective = match (posted_access, status.mount_access) {
                (Access::ReadWrite, Access::ReadWrite) => Access::ReadWrite,
                _ => Access::ReadOnly,
            };
            self.fileserver_manager
                .mount_into_live_namespace(&status.service_id, &self.root_namespace, effective)
                .await
                .map_err(|err| err.to_string())?;
            self.fileserver_manager
                .mount_into_live_namespace(&status.service_id, &self.system_namespace, effective)
                .await
                .map_err(|err| err.to_string())?;
            self.local_entry
                .mount_service(&status.mount_path, transport, effective);
            Ok::<Access, String>(effective)
        }
        .await;

        let effective = match mount_result {
            Ok(effective) => effective,
            Err(err) => {
                if started_fresh {
                    let _ = self.fileserver_manager.stop(&status.service_id).await;
                }
                return Err(err);
            }
        };

        if persist_for_root {
            self.packages
                .bind_for_root(
                    &available.package_id,
                    RootBindingMode::Active,
                    Some(RootFileserverActivation {
                        access: kernel_to_fileserver_access(effective),
                        allow_network: status.allow_network,
                        host_grant_ids: Vec::new(),
                    }),
                )
                .map_err(|err| err.to_string())?;
        }

        Ok(ActivatedServiceMount {
            package_id: available.package_id,
            service_id: status.service_id,
            mount_path: status.mount_path,
            description: available.description.unwrap_or_default(),
            access: kernel_to_service_access(effective),
            allow_network: status.allow_network,
            persisted: persist_for_root,
        })
    }
}

const fn service_to_fileserver_access(access: ServiceMountAccess) -> FileserverMountAccess {
    match access {
        ServiceMountAccess::ReadOnly => FileserverMountAccess::ReadOnly,
        ServiceMountAccess::ReadWrite => FileserverMountAccess::ReadWrite,
    }
}

const fn kernel_to_fileserver_access(access: Access) -> FileserverMountAccess {
    match access {
        Access::ReadOnly => FileserverMountAccess::ReadOnly,
        Access::ReadWrite => FileserverMountAccess::ReadWrite,
    }
}

const fn kernel_to_service_access(access: Access) -> ServiceMountAccess {
    match access {
        Access::ReadOnly => ServiceMountAccess::ReadOnly,
        Access::ReadWrite => ServiceMountAccess::ReadWrite,
    }
}
