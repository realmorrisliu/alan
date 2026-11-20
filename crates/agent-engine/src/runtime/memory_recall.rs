//! Pure-Rust BM25 + CJK + English memory recall over domain-scoped stores.
//!
//! Scans safe markdown/text memory files inside the active domain directory,
//! computes BM25 (`k1 = 1.5`, `b = 0.75`) over English/ASCII stemmed tokens and
//! CJK character bigrams + unigram fallback with IDF across scanned documents,
//! applies recency tie-breaking, and scrubs secrets with
//! [`crate::redact_secret_tokens`] on all recalled excerpts.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::redact_secret_tokens;
use crate::tape::{ContentPart, parts_to_text};

use super::memory_domains::{DomainRecallScope, resolve_domain_store_dir};
mod tokenizer;
use tokenizer::{aggregate_query_terms, excerpt_around_match, tokenize_with_weights};

const MAX_FILE_RECALL_CHARS: usize = 1_200;
const MAX_RECALL_FILES: usize = 4;
const MAX_CANDIDATE_SCAN_FILES: usize = 64;
const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;
const SCORE_TIE_EPSILON: f64 = 1e-9;

/// One ranked search hit returned across a Memory Store or explicit domain scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySearchHit {
    /// Canonical domain label (`default`, `personal`, `continuity`, `app:<id>`, `project:<slug>`).
    pub domain: String,
    /// Path relative to the domain store root (using `/` separators).
    pub relative_path: String,
    /// BM25 relevance score.
    pub score: f64,
    /// Monotonic recency score (path date + modified timestamp) used for tie-breaking.
    pub recency: u64,
    /// Secret-scrubbed bounded excerpt (`<= 1_200` chars).
    pub snippet: String,
}

#[derive(Debug, Clone)]
struct ScannedDoc {
    domain: String,
    path: PathBuf,
    relative_path: String,
    scrubbed_content: String,
    recency: u64,
    tf: HashMap<String, f64>,
    length: f64,
}

#[derive(Debug, Clone)]
struct ScoredDoc {
    doc: ScannedDoc,
    score: f64,
}

#[allow(
    dead_code,
    reason = "Preserved compatibility entry point used by existing recall tests without project cwd"
)]
pub(crate) fn build_turn_recall_bundle(
    memory_dir: Option<&Path>,
    user_input: Option<&[ContentPart]>,
) -> Option<String> {
    build_turn_recall_bundle_for_project(memory_dir, user_input, None)
}

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "project-only wrapper preserved for memory recall unit tests"
    )
)]
pub(crate) fn build_turn_recall_bundle_for_project(
    memory_dir: Option<&Path>,
    user_input: Option<&[ContentPart]>,
    project_cwd: Option<&str>,
) -> Option<String> {
    build_turn_recall_bundle_for_turn(memory_dir, user_input, project_cwd, None)
}

/// Build the per-turn runtime memory recall bundle, omitting `USER.md`, `MEMORY.md`,
/// and `handoffs/LATEST.md` when they are already injected verbatim in `## Memory Store Bootstrap`.
pub(crate) fn build_turn_recall_bundle_for_turn(
    memory_dir: Option<&Path>,
    user_input: Option<&[ContentPart]>,
    project_cwd: Option<&str>,
    memory_bootstrap_section: Option<&str>,
) -> Option<String> {
    let memory_dir = memory_dir?;
    if !memory_dir.exists() {
        return None;
    }
    let canonical_memory_root = fs::canonicalize(memory_dir).ok()?;

    let query = user_input
        .map(parts_to_text)
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.is_empty() {
        return None;
    }

    let query_lower = query.to_lowercase();
    let query_terms = aggregate_query_terms(&query);
    let identity_query = is_identity_query(&query_lower);
    let continuity_query = is_continuity_query(&query_lower);
    let domain_query = is_domain_query(&query_lower);
    let recent_query = is_recent_query(&query_lower);

    if !identity_query
        && !continuity_query
        && !domain_query
        && !recent_query
        && query_terms.is_empty()
    {
        return None;
    }

    let user_md_path = memory_dir.join("USER.md");
    let memory_md_path = memory_dir.join("MEMORY.md");
    let latest_handoff_path = memory_dir.join("handoffs").join("LATEST.md");
    let user_bootstrapped =
        is_bootstrapped_verbatim(memory_bootstrap_section, &user_md_path, "USER.md");
    let memory_bootstrapped =
        is_bootstrapped_verbatim(memory_bootstrap_section, &memory_md_path, "MEMORY.md");
    let latest_handoff_bootstrapped = is_bootstrapped_verbatim(
        memory_bootstrap_section,
        &latest_handoff_path,
        "handoffs/LATEST.md",
    );

    let mut selected_paths = Vec::new();
    if identity_query && !user_bootstrapped && is_non_stub_markdown(&user_md_path, "# User Memory")
    {
        selected_paths.push(user_md_path);
    }
    if domain_query && !memory_bootstrapped && is_non_stub_markdown(&memory_md_path, "# Memory") {
        selected_paths.push(memory_md_path);
    }
    if continuity_query {
        if !latest_handoff_bootstrapped
            && is_non_stub_markdown(&latest_handoff_path, "# Latest Handoff")
        {
            selected_paths.push(latest_handoff_path);
        }
        if let Some(summary_md) =
            super::memory_surfaces::project_summary::active_summary_markdown_path(
                memory_dir,
                project_cwd,
            )
        {
            selected_paths.push(summary_md);
        }
    }

    let episodic_paths = collect_markdown_files_recursive(
        &memory_dir.join("episodic"),
        &canonical_memory_root,
        MAX_CANDIDATE_SCAN_FILES,
    );
    let daily_paths = collect_markdown_files(
        &memory_dir.join("daily"),
        &canonical_memory_root,
        MAX_CANDIDATE_SCAN_FILES,
    );

    let has_non_temporal_query_token = query_terms.keys().any(|term| !is_temporal_only_token(term));
    let allow_topics = domain_query || !recent_query || has_non_temporal_query_token;

    let mut candidate_paths = Vec::new();
    candidate_paths.extend(episodic_paths.iter().cloned());
    candidate_paths.extend(daily_paths.iter().cloned());

    if allow_topics {
        let topics = collect_markdown_files(
            &memory_dir.join("topics"),
            &canonical_memory_root,
            MAX_CANDIDATE_SCAN_FILES,
        )
        .into_iter()
        .filter(|path| {
            super::memory_surfaces::project_summary::should_include_topic_markdown(
                memory_dir,
                path,
                project_cwd,
            )
        });
        candidate_paths.extend(topics);
    }

    // Include root-level authored markdown (USER.md, MEMORY.md, custom *.md) and handoffs when non-stub
    // and not already injected verbatim into `## Memory Store Bootstrap`.
    for path in collect_markdown_files(memory_dir, &canonical_memory_root, MAX_CANDIDATE_SCAN_FILES)
    {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if (file_name == "USER.md"
            && (user_bootstrapped || !is_non_stub_markdown(&path, "# User Memory")))
            || (file_name == "MEMORY.md"
                && (memory_bootstrapped || !is_non_stub_markdown(&path, "# Memory")))
        {
            continue;
        }
        candidate_paths.push(path);
    }
    let latest_handoff = memory_dir.join("handoffs").join("LATEST.md");
    if !latest_handoff_bootstrapped
        && is_non_stub_markdown(&latest_handoff, "# Latest Handoff")
        && path_is_safe_recall_file(&latest_handoff, &canonical_memory_root)
    {
        candidate_paths.push(latest_handoff);
    }

    let docs = load_scanned_docs(
        memory_dir,
        "active",
        &canonical_memory_root,
        candidate_paths,
        true,
    );
    let ranked = score_docs_bm25(docs, &query_terms);

    let mut selected_candidate_paths = Vec::new();
    let mut seen_candidate_paths = BTreeSet::new();
    for scored in ranked {
        if selected_candidate_paths.len() >= MAX_RECALL_FILES {
            break;
        }
        if seen_candidate_paths.insert(scored.doc.path.clone()) {
            selected_candidate_paths.push(scored.doc.path);
        }
    }

    if recent_query && selected_candidate_paths.len() < MAX_RECALL_FILES {
        for path in interleave_recent_fallback_paths(&daily_paths, &episodic_paths) {
            if selected_candidate_paths.len() >= MAX_RECALL_FILES {
                break;
            }
            if seen_candidate_paths.insert(path.clone()) {
                selected_candidate_paths.push(path);
            }
        }
    }
    selected_paths.extend(selected_candidate_paths);

    let mut seen = BTreeSet::new();
    let sections: Vec<String> = selected_paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .filter(|path| path_is_safe_recall_file(path, &canonical_memory_root))
        .filter_map(|path| {
            let content = fs::read_to_string(&path).ok()?;
            let scrubbed = redact_secret_tokens(&content);
            let trimmed = scrubbed.trim();
            if trimmed.is_empty() {
                return None;
            }
            let relative_path = format_relative_memory_path(memory_dir, &path);
            let snippet = excerpt_around_match(trimmed, &query, MAX_FILE_RECALL_CHARS);
            Some(format!("### {relative_path}\n{snippet}\n"))
        })
        .collect();

    if sections.is_empty() {
        return None;
    }

    Some(format!(
        "## Runtime Recall Bundle\n\
Selected turn-relevant pure-text memory based on the current user request. Treat this as runtime-routed recall, not raw speculative search.\n\n{}",
        sections.join("\n")
    ))
}

/// Build a turn recall bundle scoped strictly to explicitly authorized domains.
///
/// Project and app domains never bleed into automatic recall unless included in `scope`.
pub fn build_turn_recall_bundle_for_domains(
    stores_root: &Path,
    scope: &DomainRecallScope,
    user_input: Option<&[ContentPart]>,
    project_cwd: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let query = user_input
        .map(parts_to_text)
        .unwrap_or_default()
        .trim()
        .to_string();
    if query.is_empty() {
        return Ok(None);
    }

    let domains = scope.resolved_domains();
    if domains.len() == 1 {
        let dir = resolve_domain_store_dir(stores_root, &domains[0])?;
        return Ok(build_turn_recall_bundle_for_project(
            Some(&dir),
            user_input,
            project_cwd,
        ));
    }

    let hits = search_memory_domains(stores_root, scope, &query, project_cwd, MAX_RECALL_FILES)?;
    if hits.is_empty() {
        return Ok(None);
    }

    let sections: Vec<String> = hits
        .into_iter()
        .map(|hit| {
            format!(
                "### /memory/{}/{}\n{}\n",
                hit.domain, hit.relative_path, hit.snippet
            )
        })
        .collect();
    Ok(Some(format!(
        "## Runtime Recall Bundle\n\
Selected turn-relevant pure-text memory based on the current user request. Treat this as runtime-routed recall, not raw speculative search.\n\n{}",
        sections.join("\n")
    )))
}

/// Search one Memory Store directory using BM25 + CJK + English tokenization with
/// secret scrubbing applied to every snippet.
pub fn search_memory_store(
    memory_dir: &Path,
    domain_label: &str,
    query: &str,
    project_cwd: Option<&str>,
    limit: usize,
) -> Vec<MemorySearchHit> {
    if limit == 0 || query.trim().is_empty() || !memory_dir.exists() {
        return Vec::new();
    }
    let Ok(canonical_root) = fs::canonicalize(memory_dir) else {
        return Vec::new();
    };
    let query_terms = aggregate_query_terms(query);
    if query_terms.is_empty() {
        return Vec::new();
    }

    let candidate_paths =
        collect_searchable_files_for_store(memory_dir, &canonical_root, project_cwd);
    let docs = load_scanned_docs(
        memory_dir,
        domain_label,
        &canonical_root,
        candidate_paths,
        false,
    );
    let ranked = score_docs_bm25(docs, &query_terms);
    ranked
        .into_iter()
        .take(limit)
        .map(|scored| MemorySearchHit {
            domain: scored.doc.domain,
            relative_path: scored.doc.relative_path,
            score: scored.score,
            recency: scored.doc.recency,
            snippet: excerpt_around_match(
                scored.doc.scrubbed_content.trim(),
                query,
                MAX_FILE_RECALL_CHARS,
            ),
        })
        .collect()
}

/// Search across ONLY the domains explicitly included in `scope`.
pub fn search_memory_domains(
    stores_root: &Path,
    scope: &DomainRecallScope,
    query: &str,
    project_cwd: Option<&str>,
    limit: usize,
) -> anyhow::Result<Vec<MemorySearchHit>> {
    if limit == 0 || query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let query_terms = aggregate_query_terms(query);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_docs = Vec::new();
    for domain in scope.resolved_domains() {
        let dir = resolve_domain_store_dir(stores_root, &domain)?;
        if !dir.exists() {
            continue;
        }
        let Ok(canonical_root) = fs::canonicalize(&dir) else {
            continue;
        };
        let label = domain.label();
        let candidates = collect_searchable_files_for_store(&dir, &canonical_root, project_cwd);
        all_docs.extend(load_scanned_docs(
            &dir,
            &label,
            &canonical_root,
            candidates,
            false,
        ));
    }

    let ranked = score_docs_bm25(all_docs, &query_terms);
    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|scored| MemorySearchHit {
            domain: scored.doc.domain,
            relative_path: scored.doc.relative_path,
            score: scored.score,
            recency: scored.doc.recency,
            snippet: excerpt_around_match(
                scored.doc.scrubbed_content.trim(),
                query,
                MAX_FILE_RECALL_CHARS,
            ),
        })
        .collect())
}

fn load_scanned_docs(
    memory_dir: &Path,
    domain: &str,
    canonical_memory_root: &Path,
    paths: Vec<PathBuf>,
    markdown_only: bool,
) -> Vec<ScannedDoc> {
    let mut seen = BTreeSet::new();
    let mut docs = Vec::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        let safe = if markdown_only {
            path_is_safe_recall_file(&path, canonical_memory_root)
        } else {
            path_is_safe_text_file(&path, canonical_memory_root)
        };
        if !safe {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let scrubbed_content = redact_secret_tokens(&content);
        if scrubbed_content.trim().is_empty() {
            continue;
        }
        let relative = path
            .strip_prefix(memory_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));

        let indexed_text = format!("{relative}\n{scrubbed_content}");
        let mut tf: HashMap<String, f64> = HashMap::new();
        let mut length = 0.0_f64;
        for (term, weight) in tokenize_with_weights(&indexed_text) {
            length += weight;
            *tf.entry(term).or_insert(0.0) += weight;
        }
        let recency = compute_file_recency(&path, &relative);

        docs.push(ScannedDoc {
            domain: domain.to_string(),
            path,
            relative_path: relative,
            scrubbed_content,
            recency,
            tf,
            length,
        });
    }
    docs
}

fn score_docs_bm25(docs: Vec<ScannedDoc>, query_terms: &BTreeMap<String, f64>) -> Vec<ScoredDoc> {
    if docs.is_empty() || query_terms.is_empty() {
        return Vec::new();
    }

    let mut doc_freq: HashMap<String, usize> = HashMap::new();
    let mut total_len = 0.0_f64;
    for doc in &docs {
        total_len += doc.length;
        for term in doc.tf.keys() {
            *doc_freq.entry(term.clone()).or_insert(0) += 1;
        }
    }
    let doc_count = docs.len() as f64;
    let avg_doc_len = (total_len / doc_count).max(1.0);

    let mut scored = Vec::new();
    for doc in docs {
        let norm = 1.0 - BM25_B + BM25_B * (doc.length / avg_doc_len);
        let mut score = 0.0_f64;
        for (term, query_weight) in query_terms {
            let Some(&tf) = doc.tf.get(term) else {
                continue;
            };
            if tf <= 0.0 {
                continue;
            }
            let df = *doc_freq.get(term).unwrap_or(&1) as f64;
            // Lucene smoothed non-negative IDF avoids zero/negative scores on small 1..2-doc stores.
            let idf = (1.0 + (doc_count - df + 0.5) / (df + 0.5)).ln();
            let denom = tf + BM25_K1 * norm;
            if denom > 0.0 {
                score += query_weight * idf * (tf * (BM25_K1 + 1.0)) / denom;
            }
        }
        if score > 0.0 {
            scored.push(ScoredDoc { doc, score });
        }
    }

    scored.sort_by(|left, right| {
        let diff = right.score - left.score;
        if diff.abs() > SCORE_TIE_EPSILON {
            right.score.total_cmp(&left.score)
        } else {
            right
                .doc
                .recency
                .cmp(&left.doc.recency)
                .then_with(|| right.doc.path.cmp(&left.doc.path))
        }
    });
    scored
}

fn collect_searchable_files_for_store(
    memory_dir: &Path,
    canonical_memory_root: &Path,
    project_cwd: Option<&str>,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_store_files_recursive(
        memory_dir,
        memory_dir,
        canonical_memory_root,
        project_cwd,
        &mut files,
        MAX_CANDIDATE_SCAN_FILES * 4,
    );
    files.sort();
    files.reverse();
    files
}

fn collect_store_files_recursive(
    dir: &Path,
    memory_dir: &Path,
    canonical_memory_root: &Path,
    project_cwd: Option<&str>,
    out: &mut Vec<PathBuf>,
    limit: usize,
) {
    if out.len() >= limit || !path_is_within_canonical_root(dir, canonical_memory_root) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    paths.reverse();

    for path in paths {
        if out.len() >= limit {
            break;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if file_type.is_dir() {
            if matches!(name, ".revisions" | "inbox" | "working") || name.starts_with('.') {
                continue;
            }
            collect_store_files_recursive(
                &path,
                memory_dir,
                canonical_memory_root,
                project_cwd,
                out,
                limit,
            );
        } else if file_type.is_file() && path_is_safe_text_file(&path, canonical_memory_root) {
            if name.ends_with(".json") || name.ends_with(".lock") || name.starts_with('.') {
                continue;
            }
            if path.parent() == Some(&memory_dir.join("topics"))
                && name.ends_with(".md")
                && !super::memory_surfaces::project_summary::should_include_topic_markdown(
                    memory_dir,
                    &path,
                    project_cwd,
                )
            {
                continue;
            }
            out.push(path);
        }
    }
}

fn is_non_stub_markdown(path: &Path, stub_header: &str) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let trimmed = content.trim();
    !trimmed.is_empty() && trimmed != stub_header
}

fn is_bootstrapped_verbatim(
    memory_bootstrap_section: Option<&str>,
    path: &Path,
    label: &str,
) -> bool {
    let Some(section) = memory_bootstrap_section else {
        return false;
    };
    if !section.contains("## Memory Store Bootstrap") {
        return false;
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > crate::prompts::MEMORY_STORE_MAX_CHARS {
        return false;
    }
    let marker = format!("### {label}\n");
    let Some(start) = section.find(&marker) else {
        return false;
    };
    let after = &section[start + marker.len()..];
    let body = match after.find("\n### ") {
        Some(next) => &after[..next],
        None => after,
    };
    body.contains(trimmed) && !body.contains("[...truncated,")
}

fn compute_file_recency(path: &Path, relative_path: &str) -> u64 {
    let date_key = extract_yyyymmdd_from_path(relative_path).unwrap_or(0);
    let mtime_secs = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|dur| dur.as_secs().min(u32::MAX as u64))
        .unwrap_or(0);
    (date_key << 32) | mtime_secs
}

fn extract_yyyymmdd_from_path(relative_path: &str) -> Option<u64> {
    let bytes = relative_path.as_bytes();
    for start in 0..bytes.len() {
        if start + 10 <= bytes.len() {
            let window = &bytes[start..start + 10];
            if window[..4].iter().all(u8::is_ascii_digit)
                && matches!(window[4], b'-' | b'/')
                && window[5..7].iter().all(u8::is_ascii_digit)
                && matches!(window[7], b'-' | b'/')
                && window[8..10].iter().all(u8::is_ascii_digit)
                && let Ok(year) = std::str::from_utf8(&window[..4])
                    .unwrap_or("")
                    .parse::<u64>()
                && let Ok(month) = std::str::from_utf8(&window[5..7])
                    .unwrap_or("")
                    .parse::<u64>()
                && let Ok(day) = std::str::from_utf8(&window[8..10])
                    .unwrap_or("")
                    .parse::<u64>()
            {
                return Some(year * 10_000 + month * 100 + day);
            }
        }
    }
    None
}

fn format_relative_memory_path(memory_dir: &Path, path: &Path) -> String {
    path.strip_prefix(memory_dir)
        .map(|relative| {
            let relative = relative.to_string_lossy().replace('\\', "/");
            format!("/memory/{relative}")
        })
        .unwrap_or_else(|_| "/memory".to_string())
}

fn path_is_within_canonical_root(path: &Path, canonical_memory_root: &Path) -> bool {
    fs::canonicalize(path)
        .ok()
        .is_some_and(|canonical_path| canonical_path.starts_with(canonical_memory_root))
}

fn path_is_safe_recall_file(path: &Path, canonical_memory_root: &Path) -> bool {
    path_is_safe_text_file(path, canonical_memory_root)
        && path
            .extension()
            .is_some_and(|extension| extension == std::ffi::OsStr::new("md"))
}

fn path_is_safe_text_file(path: &Path, canonical_memory_root: &Path) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_file())
        && path_is_within_canonical_root(path, canonical_memory_root)
}

fn interleave_recent_fallback_paths(
    daily_paths: &[PathBuf],
    episodic_paths: &[PathBuf],
) -> Vec<PathBuf> {
    let mut interleaved = Vec::with_capacity(daily_paths.len() + episodic_paths.len());
    let max_len = daily_paths.len().max(episodic_paths.len());
    for index in 0..max_len {
        if let Some(path) = daily_paths.get(index) {
            interleaved.push(path.clone());
        }
        if let Some(path) = episodic_paths.get(index) {
            interleaved.push(path.clone());
        }
    }
    interleaved
}

fn is_identity_query(query: &str) -> bool {
    [
        "who am i",
        "my name",
        "my preference",
        "my preferences",
        "favorite",
        "prefer",
        "我是谁",
        "我的名字",
        "我叫什么",
        "我的偏好",
        "用户偏好",
        "偏好",
        "喜欢",
        "习惯",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_continuity_query(query: &str) -> bool {
    [
        "continue",
        "resume",
        "last time",
        "previous agent process",
        "earlier",
        "before",
        "where did we leave off",
        "what were we doing",
        "继续",
        "上次",
        "之前",
        "刚才",
        "恢复",
        "接着",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_domain_query(query: &str) -> bool {
    [
        "architecture",
        "decision",
        "constraint",
        "project context",
        "why did we",
        "workspace",
        "架构",
        "决策",
        "约束",
        "项目上下文",
        "为什么",
        "工作区",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_recent_query(query: &str) -> bool {
    [
        "today",
        "yesterday",
        "recent",
        "latest",
        "just now",
        "今天",
        "昨天",
        "最近",
        "刚刚",
        "最新",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_temporal_only_token(term: &str) -> bool {
    matches!(
        term,
        "today"
            | "yesterday"
            | "recent"
            | "latest"
            | "now"
            | "今天"
            | "昨天"
            | "最近"
            | "刚刚"
            | "最新"
    )
}

fn collect_markdown_files(
    dir: &Path,
    canonical_memory_root: &Path,
    max_files: usize,
) -> Vec<PathBuf> {
    if !path_is_within_canonical_root(dir, canonical_memory_root) {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if file_type.is_symlink() || !file_type.is_file() {
                return None;
            }
            let path = entry.path();
            path_is_safe_recall_file(&path, canonical_memory_root).then_some(path)
        })
        .collect();
    files.sort();
    files.reverse();
    files.truncate(max_files);
    files
}

fn collect_markdown_files_recursive(
    dir: &Path,
    canonical_memory_root: &Path,
    max_files: usize,
) -> Vec<PathBuf> {
    let mut collected = Vec::new();
    collect_markdown_files_recursive_inner(dir, canonical_memory_root, &mut collected, max_files);
    collected.sort();
    collected.reverse();
    collected.truncate(max_files);
    collected
}

fn collect_markdown_files_recursive_inner(
    dir: &Path,
    canonical_memory_root: &Path,
    collected: &mut Vec<PathBuf>,
    max_files: usize,
) {
    if collected.len() >= max_files || !path_is_within_canonical_root(dir, canonical_memory_root) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths.reverse();
    for path in paths {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_markdown_files_recursive_inner(
                &path,
                canonical_memory_root,
                collected,
                max_files,
            );
        } else if file_type.is_file() && path_is_safe_recall_file(&path, canonical_memory_root) {
            collected.push(path);
            if collected.len() >= max_files {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "memory_recall_tests.rs"]
mod tests;
