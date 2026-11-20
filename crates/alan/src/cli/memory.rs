//! Domain-scoped Memory Store CLI (`alan memory list|show|edit|revert`).
//!
//! Operates strictly within `SystemStorePaths::memory_stores()` (never implicit
//! Host-directory stores), enforces cross-domain isolation, scrubs secrets via
//! `alan_agent_engine::redact_secret_tokens`, and atomically snapshots prior
//! versions in store-local `.revisions/` before updating or reverting.

mod revisions;

use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use alan_agent_engine::{
    DomainRecallScope, MemoryDomain, MemorySearchHit, list_existing_memory_domains,
    redact_secret_tokens, resolve_domain_store_dir, search_memory_domains,
};
use alan_os_host::SystemStorePaths;
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

pub use revisions::MemoryRevisionEntry;
use revisions::{
    collect_domain_files, ensure_domain_dir_is_real, ensure_safe_existing_file,
    ensure_safe_writable_target, load_history, normalize_relative_memory_path, record_snapshot,
    revision_target_dir, sha256_hex, single_line_preview, write_atomic,
};

const MAX_EDIT_BYTES: usize = 1 << 20; // 1 MiB
const DEFAULT_SEARCH_LIMIT: usize = 50;

/// Top-level arguments for `alan memory`.
#[derive(Debug, Args)]
pub struct MemoryArgs {
    /// Memory Store subcommand to run.
    #[command(subcommand)]
    pub action: MemoryAction,
}

/// Supported `alan memory` subcommands.
#[derive(Debug, Subcommand)]
pub enum MemoryAction {
    /// List memory files in a domain, or search them with BM25 + CJK ranking.
    List {
        /// Domain to inspect (`default`, `personal`, `continuity`, `app:<id>`, `project:<slug>`, or `all`).
        #[arg(long, value_name = "DOMAIN")]
        domain: Option<String>,
        /// Optional BM25 + CJK search query (ranked with secret-scrubbed snippets).
        #[arg(long, value_name = "QUERY")]
        query: Option<String>,
        /// Emit structured JSON instead of human-readable text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show sanitized memory content and recorded revision history for one file.
    Show {
        /// Relative memory path or file name inside the domain store (e.g. `USER.md` or `topics/notes.md`).
        #[arg(value_name = "PATH_OR_NAME")]
        target: String,
        /// Target Memory Domain (defaults to `default`).
        #[arg(long, value_name = "DOMAIN")]
        domain: Option<String>,
        /// Emit structured JSON instead of plain text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Edit or create a memory file after atomically snapshotting any prior version.
    Edit {
        /// Relative memory path or file name inside the domain store (e.g. `USER.md` or `topics/notes.md`).
        #[arg(value_name = "PATH_OR_NAME")]
        target: String,
        /// Inline UTF-8 content to write (scrubbed for secret tokens before saving).
        #[arg(long, value_name = "TEXT", conflicts_with = "file")]
        content: Option<String>,
        /// Local UTF-8 file whose contents replace the memory file (scrubbed before saving).
        #[arg(long, value_name = "PATH", conflicts_with = "content")]
        file: Option<PathBuf>,
        /// Target Memory Domain (defaults to `default`).
        #[arg(long, value_name = "DOMAIN")]
        domain: Option<String>,
        /// Emit structured JSON instead of plain text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Revert a memory file to a previously snapshotted revision in `.revisions/`.
    Revert {
        /// Relative memory path or file name inside the domain store.
        #[arg(value_name = "PATH_OR_NAME")]
        target: String,
        /// Specific revision id to restore (defaults to the most recent snapshot).
        #[arg(long, value_name = "ID")]
        revision: Option<String>,
        /// Target Memory Domain (defaults to `default`).
        #[arg(long, value_name = "DOMAIN")]
        domain: Option<String>,
        /// Emit structured JSON instead of plain text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Entry returned when listing files without `--query`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryListEntry {
    pub domain: String,
    pub relative_path: String,
    pub bytes: usize,
    pub revisions: usize,
}

#[derive(Debug, Serialize)]
struct MemoryListJsonReport {
    domains: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    entries: Vec<MemoryListEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hits: Vec<MemorySearchHit>,
}

/// Structured view returned by `alan memory show --json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryShowReport {
    pub domain: String,
    pub relative_path: String,
    pub bytes: usize,
    pub sha256: String,
    pub content: String,
    pub revisions: Vec<MemoryRevisionEntry>,
}

/// Structured view returned by `alan memory edit --json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEditReport {
    pub domain: String,
    pub relative_path: String,
    pub bytes: usize,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshotted_revision: Option<MemoryRevisionEntry>,
}

/// Structured view returned by `alan memory revert --json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRevertReport {
    pub domain: String,
    pub relative_path: String,
    pub restored_revision: MemoryRevisionEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_revert_snapshot: Option<MemoryRevisionEntry>,
}

/// Execute `alan memory ...` against the current installation's System Store.
pub async fn run(args: MemoryArgs) -> Result<()> {
    alan_os_host::installation::validate_current_invocation()?;
    let _installation_access = alan_os_host::installation::InstallationPaths::detect()?.access()?;
    let system = SystemStorePaths::detect()?;
    let stores_root = system.memory_stores()?;
    let stdin_content = read_piped_stdin_if_needed(&args.action)?;
    let output = run_with_stores_root(&stores_root, args, stdin_content)?;
    print!("{output}");
    Ok(())
}

/// Execute `MemoryArgs` against an explicit `stores_root` directory (for CLI and hermetic tests).
pub fn run_with_stores_root(
    stores_root: &Path,
    args: MemoryArgs,
    stdin_content: Option<String>,
) -> Result<String> {
    match args.action {
        MemoryAction::List {
            domain,
            query,
            json,
        } => run_list(stores_root, domain.as_deref(), query.as_deref(), json),
        MemoryAction::Show {
            target,
            domain,
            json,
        } => run_show(stores_root, domain.as_deref(), &target, json),
        MemoryAction::Edit {
            target,
            content,
            file,
            domain,
            json,
        } => {
            let resolved_content = resolve_edit_content(content, file.as_deref(), stdin_content)?;
            run_edit(
                stores_root,
                domain.as_deref(),
                &target,
                &resolved_content,
                json,
            )
        }
        MemoryAction::Revert {
            target,
            revision,
            domain,
            json,
        } => run_revert(
            stores_root,
            domain.as_deref(),
            &target,
            revision.as_deref(),
            json,
        ),
    }
}

fn read_piped_stdin_if_needed(action: &MemoryAction) -> Result<Option<String>> {
    let MemoryAction::Edit {
        content: None,
        file: None,
        ..
    } = action
    else {
        return Ok(None);
    };
    if std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let mut buf = Vec::new();
    std::io::stdin()
        .take(MAX_EDIT_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .context("read memory content from stdin")?;
    ensure!(
        buf.len() <= MAX_EDIT_BYTES,
        "stdin memory content exceeds {MAX_EDIT_BYTES} bytes"
    );
    let text = String::from_utf8(buf).context("stdin memory content must be valid UTF-8")?;
    Ok(Some(text))
}

fn resolve_edit_content(
    content: Option<String>,
    file: Option<&Path>,
    stdin_content: Option<String>,
) -> Result<String> {
    if let Some(text) = content {
        ensure!(
            text.len() <= MAX_EDIT_BYTES,
            "inline memory content exceeds {MAX_EDIT_BYTES} bytes"
        );
        return Ok(text);
    }
    if let Some(file_path) = file {
        let meta = fs::symlink_metadata(file_path)
            .with_context(|| format!("inspect input file {}", file_path.display()))?;
        ensure!(
            !meta.file_type().is_symlink() && meta.is_file(),
            "input file must be a regular non-symlink file: {}",
            file_path.display()
        );
        ensure!(
            meta.len() <= MAX_EDIT_BYTES as u64,
            "input file exceeds {MAX_EDIT_BYTES} bytes: {}",
            file_path.display()
        );
        return fs::read_to_string(file_path)
            .with_context(|| format!("read UTF-8 input file {}", file_path.display()));
    }
    if let Some(text) = stdin_content {
        return Ok(text);
    }
    bail!("`alan memory edit` requires `--content <text>`, `--file <path>`, or piped stdin")
}

fn resolve_list_domains(stores_root: &Path, domain_arg: Option<&str>) -> Result<Vec<MemoryDomain>> {
    match domain_arg.map(str::trim) {
        None => Ok(vec![MemoryDomain::Default]),
        Some("all") => list_existing_memory_domains(stores_root),
        Some(raw) => Ok(vec![MemoryDomain::parse(raw)?]),
    }
}

fn parse_single_domain(domain_arg: Option<&str>) -> Result<MemoryDomain> {
    match domain_arg.map(str::trim) {
        None => Ok(MemoryDomain::Default),
        Some("all") => bail!("`--domain all` is only supported for `alan memory list`"),
        Some(raw) => MemoryDomain::parse(raw),
    }
}

fn run_list(
    stores_root: &Path,
    domain_arg: Option<&str>,
    query: Option<&str>,
    json: bool,
) -> Result<String> {
    let domains = resolve_list_domains(stores_root, domain_arg)?;
    let domain_labels: Vec<String> = domains.iter().map(MemoryDomain::label).collect();

    if let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) {
        let scope = DomainRecallScope::Explicit(domains);
        let hits = search_memory_domains(stores_root, &scope, query, None, DEFAULT_SEARCH_LIMIT)?;
        if json {
            let report = MemoryListJsonReport {
                domains: domain_labels,
                query: Some(query.to_string()),
                entries: Vec::new(),
                hits,
            };
            return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
        }
        if hits.is_empty() {
            return Ok(format!(
                "No memory matches for `{query}` in domain(s): {}.\n",
                domain_labels.join(", ")
            ));
        }
        let mut out = String::new();
        for hit in hits {
            let preview = single_line_preview(&hit.snippet, 120);
            out.push_str(&format!(
                "[{}] {} (score={:.3}) {}\n",
                hit.domain, hit.relative_path, hit.score, preview
            ));
        }
        return Ok(out);
    }

    let mut entries = Vec::new();
    for domain in &domains {
        let domain_dir = resolve_domain_store_dir(stores_root, domain)?;
        if !domain_dir.exists() {
            continue;
        }
        let canonical_domain = fs::canonicalize(&domain_dir)?;
        let label = domain.label();
        let mut rel_paths = Vec::new();
        collect_domain_files(&domain_dir, &domain_dir, &canonical_domain, &mut rel_paths)?;
        rel_paths.sort();
        for rel_path in rel_paths {
            let abs = domain_dir.join(&rel_path);
            let bytes = fs::metadata(&abs)?.len() as usize;
            let revisions = load_history(&domain_dir, &rel_path)?.entries.len();
            entries.push(MemoryListEntry {
                domain: label.clone(),
                relative_path: rel_path,
                bytes,
                revisions,
            });
        }
    }

    if json {
        let report = MemoryListJsonReport {
            domains: domain_labels,
            query: None,
            entries,
            hits: Vec::new(),
        };
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }

    if entries.is_empty() {
        return Ok(format!(
            "No memory files found in domain(s): {}.\n",
            domain_labels.join(", ")
        ));
    }

    let mut out = String::new();
    for entry in entries {
        out.push_str(&format!(
            "[{}] {} ({} bytes, {} rev(s))\n",
            entry.domain, entry.relative_path, entry.bytes, entry.revisions
        ));
    }
    Ok(out)
}

fn run_show(
    stores_root: &Path,
    domain_arg: Option<&str>,
    target: &str,
    json: bool,
) -> Result<String> {
    let domain = parse_single_domain(domain_arg)?;
    let domain_dir = resolve_domain_store_dir(stores_root, &domain)?;
    let rel_path = normalize_relative_memory_path(target)?;
    let target_path = ensure_safe_existing_file(&domain_dir, &rel_path)?;

    let raw = fs::read_to_string(&target_path)
        .with_context(|| format!("read UTF-8 memory file {}", target_path.display()))?;
    let sanitized = redact_secret_tokens(&raw);
    let sha256 = sha256_hex(sanitized.as_bytes());
    let history = load_history(&domain_dir, &rel_path)?;

    let report = MemoryShowReport {
        domain: domain.label(),
        relative_path: rel_path,
        bytes: sanitized.len(),
        sha256,
        content: sanitized,
        revisions: history.entries,
    };

    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }

    let mut out = format!(
        "domain: {}\npath: {}\nbytes: {}\nsha256: {}\nrevisions: {}\n",
        report.domain,
        report.relative_path,
        report.bytes,
        report.sha256,
        report.revisions.len()
    );
    for rev in &report.revisions {
        out.push_str(&format!(
            "  - {} ({}, {} bytes, {}, sha256:{})\n",
            rev.revision_id,
            rev.created_at,
            rev.bytes,
            rev.kind,
            &rev.sha256[..12.min(rev.sha256.len())]
        ));
    }
    out.push_str("---\n");
    out.push_str(&report.content);
    if !report.content.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn run_edit(
    stores_root: &Path,
    domain_arg: Option<&str>,
    target: &str,
    raw_content: &str,
    json: bool,
) -> Result<String> {
    let domain = parse_single_domain(domain_arg)?;
    let domain_dir = resolve_domain_store_dir(stores_root, &domain)?;
    let rel_path = normalize_relative_memory_path(target)?;

    fs::create_dir_all(&domain_dir)
        .with_context(|| format!("create domain directory {}", domain_dir.display()))?;
    let target_path = ensure_safe_writable_target(&domain_dir, &rel_path)?;

    let scrubbed = redact_secret_tokens(raw_content);
    let snapshotted_revision = if fs::symlink_metadata(&target_path).is_ok() {
        let existing = fs::read_to_string(&target_path)
            .with_context(|| format!("read existing memory file {}", target_path.display()))?;
        let scrubbed_existing = redact_secret_tokens(&existing);
        Some(record_snapshot(
            &domain_dir,
            &rel_path,
            &scrubbed_existing,
            "pre_edit",
        )?)
    } else {
        None
    };

    write_atomic(&target_path, scrubbed.as_bytes())?;
    let sha256 = sha256_hex(scrubbed.as_bytes());
    let report = MemoryEditReport {
        domain: domain.label(),
        relative_path: rel_path,
        bytes: scrubbed.len(),
        sha256,
        snapshotted_revision,
    };

    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }

    let snap_note = report
        .snapshotted_revision
        .as_ref()
        .map(|rev| format!(" (prior version snapshotted as {})", rev.revision_id))
        .unwrap_or_default();
    Ok(format!(
        "updated [{}] {} ({} bytes, sha256:{}){}\n",
        report.domain,
        report.relative_path,
        report.bytes,
        &report.sha256[..12],
        snap_note
    ))
}

fn run_revert(
    stores_root: &Path,
    domain_arg: Option<&str>,
    target: &str,
    revision_arg: Option<&str>,
    json: bool,
) -> Result<String> {
    let domain = parse_single_domain(domain_arg)?;
    let domain_dir = resolve_domain_store_dir(stores_root, &domain)?;
    let rel_path = normalize_relative_memory_path(target)?;
    ensure_domain_dir_is_real(&domain_dir)?;

    let history = load_history(&domain_dir, &rel_path)?;
    if history.entries.is_empty() {
        bail!(
            "no revision snapshots recorded for `{}` in domain `{}`",
            rel_path,
            domain.label()
        );
    }

    let chosen = match revision_arg.map(str::trim) {
        Some(id) => history
            .entries
            .iter()
            .find(|entry| entry.revision_id == id)
            .cloned()
            .with_context(|| {
                format!(
                    "revision `{id}` not found for `{}` in domain `{}`",
                    rel_path,
                    domain.label()
                )
            })?,
        None => history
            .entries
            .last()
            .cloned()
            .context("revision history unexpectedly empty")?,
    };

    let rev_dir = revision_target_dir(&domain_dir, &rel_path)?;
    let snap_path = ensure_safe_existing_file(&rev_dir, &chosen.snapshot_file)?;
    let snap_content = fs::read_to_string(&snap_path)
        .with_context(|| format!("read revision snapshot {}", snap_path.display()))?;
    let actual_sha = sha256_hex(snap_content.as_bytes());
    ensure!(
        actual_sha == chosen.sha256,
        "snapshot integrity mismatch for revision `{}` (expected {}, got {})",
        chosen.revision_id,
        chosen.sha256,
        actual_sha
    );

    let restored_text = redact_secret_tokens(&snap_content);
    let target_path = ensure_safe_writable_target(&domain_dir, &rel_path)?;
    let pre_revert_snapshot = if fs::symlink_metadata(&target_path).is_ok() {
        let current = fs::read_to_string(&target_path)
            .with_context(|| format!("read current file {}", target_path.display()))?;
        let scrubbed_current = redact_secret_tokens(&current);
        if scrubbed_current != restored_text {
            Some(record_snapshot(
                &domain_dir,
                &rel_path,
                &scrubbed_current,
                "pre_revert",
            )?)
        } else {
            None
        }
    } else {
        None
    };

    write_atomic(&target_path, restored_text.as_bytes())?;
    let report = MemoryRevertReport {
        domain: domain.label(),
        relative_path: rel_path,
        restored_revision: chosen,
        pre_revert_snapshot,
    };

    if json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }

    Ok(format!(
        "reverted [{}] {} to {} ({} bytes)\n",
        report.domain,
        report.relative_path,
        report.restored_revision.revision_id,
        report.restored_revision.bytes
    ))
}

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;
