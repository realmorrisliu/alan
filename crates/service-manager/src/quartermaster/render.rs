//! Output formatting for Quartermaster `/bin/q`.

use std::collections::BTreeSet;

use crate::{PackageCatalog, PackageCommandResult, PackageExportKind};

use super::QError;

/// Formats a Package Service mutation result for stdout.
pub(super) fn render_mutation(result: PackageCommandResult) -> Result<Vec<u8>, QError> {
    if !result.success {
        return Err(QError::Operation(format!(
            "{} failed: {}",
            result.action, result.message
        )));
    }
    let package = result.package;
    let detail = package
        .as_ref()
        .map(|package| format!(" {} {}", package.id, package.revision))
        .unwrap_or_default();
    Ok(format!("{}{}\n", result.message, detail).into_bytes())
}

/// Formats a Package Service catalog query result for stdout.
pub(super) fn render_catalog(result: PackageCommandResult) -> Result<Vec<u8>, QError> {
    if !result.success {
        return Err(QError::Operation(format!(
            "{} failed: {}",
            result.action, result.message
        )));
    }
    let catalog: PackageCatalog = result
        .catalog
        .ok_or_else(|| QError::Operation("catalog result has no catalog".to_string()))?;
    if catalog.packages.is_empty() {
        return Ok(b"no packages installed\n".to_vec());
    }
    let mut output = String::new();
    for record in catalog.packages.values() {
        let kind = match record.kind {
            crate::PackageKind::Preinstalled => "preinstalled",
            crate::PackageKind::Installed => "installed",
        };
        let state = match record.state {
            crate::PackageState::Installed => "installed",
            crate::PackageState::Retiring => "retiring",
        };
        let export_kind = record.export_kind.as_str();
        let exports_summary = match record.export_kind {
            PackageExportKind::Skill => {
                let skills = record
                    .exports
                    .iter()
                    .map(|export| export.skill_id.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("skills={skills}")
            }
            PackageExportKind::Tool => {
                let tools = record
                    .tool_exports
                    .iter()
                    .map(|export| export.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("tools={tools}")
            }
            PackageExportKind::Mcp => {
                let servers = record
                    .mcp_exports
                    .iter()
                    .map(|export| export.server_id.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("mcps={servers}")
            }
            PackageExportKind::Provider => {
                let detail = record
                    .provider_export
                    .as_ref()
                    .map(|provider| format!("{}({})", provider.provider_id, provider.protocol))
                    .unwrap_or_default();
                format!("provider={detail}")
            }
            PackageExportKind::Fileserver => {
                let detail = record
                    .fileserver_export
                    .as_ref()
                    .map(|fs| {
                        let transport = match &fs.transport {
                            crate::FileserverTransportSpec::Stdio { .. } => "stdio",
                            crate::FileserverTransportSpec::UnixSocket { .. } => "unix_socket",
                        };
                        format!("{}({transport})", fs.service_id)
                    })
                    .unwrap_or_default();
                format!("fileserver={detail}")
            }
        };
        let issues = record
            .exports
            .iter()
            .flat_map(|export| export.dependencies.iter())
            .map(|dependency| dependency.identity_key())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        output.push_str(&format!(
            "{} {} {} {} export={} refs={} {}",
            record.id,
            record.revision,
            kind,
            state,
            export_kind,
            record.reference_count,
            exports_summary
        ));
        if !issues.is_empty() {
            output.push_str(&format!(" unavailable={issues}"));
        }
        output.push('\n');
    }
    Ok(output.into_bytes())
}

/// Returns `/bin/q` CLI usage text.
pub(super) fn usage() -> &'static str {
    "usage: q install [--name <package-id>] <namespace-path>\n       q list\n       q upgrade <package-id> <namespace-path>\n       q uninstall <package-id>\n       q sync [[--name <package-id>] <namespace-path>]\n"
}
