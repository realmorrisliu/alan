use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::tape::{ContentPart, parts_to_text};

const MAX_FILE_RECALL_CHARS: usize = 1_200;
const MAX_RECALL_FILES: usize = 4;
const MAX_CANDIDATE_SCAN_FILES: usize = 64;

#[derive(Debug, Clone)]
struct RecallCandidate {
    path: PathBuf,
    score: usize,
}

pub(crate) fn build_turn_recall_bundle(
    memory_dir: Option<&Path>,
    user_input: Option<&[ContentPart]>,
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
    let query_tokens = tokenize_query(&query_lower);
    let identity_query = is_identity_query(&query_lower);
    let continuity_query = is_continuity_query(&query_lower);
    let workspace_query = is_workspace_query(&query_lower);
    let recent_query = is_recent_query(&query_lower);

    if !identity_query
        && !continuity_query
        && !workspace_query
        && !recent_query
        && query_tokens.is_empty()
    {
        return None;
    }

    let mut selected_paths = Vec::new();
    if identity_query {
        selected_paths.push(memory_dir.join("USER.md"));
    }
    if workspace_query {
        selected_paths.push(memory_dir.join("MEMORY.md"));
    }
    if continuity_query {
        selected_paths.push(memory_dir.join("handoffs").join("LATEST.md"));
    }

    let mut scored_candidates = Vec::new();
    let mut fallback_recent_episodic_paths = Vec::new();
    let mut fallback_recent_daily_paths = Vec::new();
    if continuity_query || recent_query {
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
        if recent_query {
            fallback_recent_episodic_paths.extend(episodic_paths.iter().cloned());
            fallback_recent_daily_paths.extend(daily_paths.iter().cloned());
        }
        scored_candidates.extend(score_candidate_files(
            episodic_paths,
            memory_dir,
            &query_tokens,
        ));
        scored_candidates.extend(score_candidate_files(
            daily_paths,
            memory_dir,
            &query_tokens,
        ));
    }
    let should_score_topics = workspace_query
        || (!identity_query && !continuity_query && !recent_query && !query_tokens.is_empty());
    if should_score_topics {
        scored_candidates.extend(score_candidate_files(
            collect_markdown_files(
                &memory_dir.join("topics"),
                &canonical_memory_root,
                MAX_CANDIDATE_SCAN_FILES,
            ),
            memory_dir,
            &query_tokens,
        ));
    }

    scored_candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.path.cmp(&left.path))
    });
    let mut selected_candidate_paths = Vec::new();
    let mut seen_candidate_paths = BTreeSet::new();
    for candidate in scored_candidates
        .into_iter()
        .filter(|candidate| candidate.score > 0)
    {
        if selected_candidate_paths.len() >= MAX_RECALL_FILES {
            break;
        }
        if seen_candidate_paths.insert(candidate.path.clone()) {
            selected_candidate_paths.push(candidate.path);
        }
    }
    if recent_query && selected_candidate_paths.len() < MAX_RECALL_FILES {
        for path in interleave_recent_fallback_paths(
            &fallback_recent_daily_paths,
            &fallback_recent_episodic_paths,
        ) {
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
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return None;
            }
            let relative_path = format_relative_memory_path(memory_dir, &path);
            Some(format!(
                "### {relative_path}\n{}\n",
                truncate_chars(trimmed, MAX_FILE_RECALL_CHARS)
            ))
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

fn format_relative_memory_path(memory_dir: &Path, path: &Path) -> String {
    path.strip_prefix(memory_dir)
        .map(|relative| {
            let relative = relative.to_string_lossy().replace('\\', "/");
            if let Some(channel_dir) = memory_dir.parent()
                && channel_dir
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name == "runtime")
                && let Some(channel_id) = channel_dir.file_name().and_then(|name| name.to_str())
            {
                return format!(".alan/runtime/{channel_id}/memory/{relative}");
            }
            format!(".alan/memory/{relative}")
        })
        .unwrap_or_else(|_| path.display().to_string())
}

fn path_is_within_canonical_root(path: &Path, canonical_memory_root: &Path) -> bool {
    fs::canonicalize(path)
        .ok()
        .is_some_and(|canonical_path| canonical_path.starts_with(canonical_memory_root))
}

fn path_is_safe_recall_file(path: &Path, canonical_memory_root: &Path) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_file())
        && path_is_within_canonical_root(path, canonical_memory_root)
        && path
            .extension()
            .is_some_and(|extension| extension == std::ffi::OsStr::new("md"))
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
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_workspace_query(query: &str) -> bool {
    [
        "architecture",
        "decision",
        "constraint",
        "project context",
        "why did we",
        "workspace",
    ]
    .iter()
    .any(|needle| query.contains(needle))
}

fn is_recent_query(query: &str) -> bool {
    ["today", "yesterday", "recent", "latest", "just now"]
        .iter()
        .any(|needle| query.contains(needle))
}

fn tokenize_query(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "about", "after", "again", "been", "from", "have", "into", "just", "that", "the", "their",
        "them", "then", "they", "this", "what", "when", "where", "which", "while", "with", "would",
        "your", "were",
    ];

    query
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 3)
        .filter(|token| !STOP_WORDS.contains(token))
        .map(ToString::to_string)
        .collect()
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
    if collected.len() >= max_files {
        return;
    }
    if !path_is_within_canonical_root(dir, canonical_memory_root) {
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

fn score_candidate_files(
    paths: Vec<PathBuf>,
    memory_dir: &Path,
    query_tokens: &[String],
) -> Vec<RecallCandidate> {
    paths
        .into_iter()
        .filter_map(|path| {
            let content = fs::read_to_string(&path).ok()?;
            let score = lexical_overlap_score(&path, memory_dir, &content, query_tokens);
            Some(RecallCandidate { path, score })
        })
        .collect()
}

fn lexical_overlap_score(
    path: &Path,
    memory_dir: &Path,
    content: &str,
    query_tokens: &[String],
) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }
    let relative_path = path.strip_prefix(memory_dir).unwrap_or(path);
    let path_text = relative_path.to_string_lossy().to_lowercase();
    let content_text = content.to_lowercase();
    query_tokens
        .iter()
        .filter(|token| path_text.contains(token.as_str()) || content_text.contains(token.as_str()))
        .count()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn identity_query_prefers_user_memory() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(
            memory_dir.join("USER.md"),
            "# User Memory\nName: Morris Liu\n",
        )
        .unwrap();

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text("Who am I?")]),
        )
        .expect("expected recall bundle");

        assert!(bundle.contains(".alan/memory/USER.md"));
        assert!(bundle.contains("Morris Liu"));
    }

    #[test]
    fn continuity_query_picks_handoff_and_episodic_record() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(
            memory_dir.join("handoffs/LATEST.md"),
            "# Latest Handoff\nWe were refining the recall router.\n",
        )
        .unwrap();
        fs::create_dir_all(memory_dir.join("episodic/2026/04/15")).unwrap();
        fs::write(
            memory_dir.join("episodic/2026/04/15/sess-1.md"),
            "# Agent Process Activity\nRecall router work in progress.\n",
        )
        .unwrap();

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text(
                "What were we doing in the previous Agent Process?",
            )]),
        )
        .expect("expected recall bundle");

        assert!(bundle.contains(".alan/memory/handoffs/LATEST.md"));
        assert!(bundle.contains(".alan/memory/episodic/2026/04/15/sess-1.md"));
    }

    #[test]
    fn workspace_query_can_pick_relevant_topic_page() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(
            memory_dir.join("topics/memory-router.md"),
            "# Memory Router\nArchitecture decision: use lexical recall over pure-text files.\n",
        )
        .unwrap();

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text(
                "What is the architecture decision for the memory router?",
            )]),
        )
        .expect("expected recall bundle");

        assert!(bundle.contains(".alan/memory/topics/memory-router.md"));
        assert!(bundle.contains("lexical recall"));
    }

    #[test]
    fn recent_query_fallback_ignores_memory_root_token_overlap() {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("yesterday-root/.alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::create_dir_all(memory_dir.join("episodic/2026/04/16")).unwrap();
        for index in 1..=4 {
            fs::write(
                memory_dir.join(format!("topics/recent-match-{index}.md")),
                format!("# Topic Note\nwe did document topic match {index}\n"),
            )
            .unwrap();
        }
        fs::write(
            memory_dir.join("daily/2026-04-16.md"),
            "## 2026-04-16\nALAN_RECENT_RECALL\n",
        )
        .unwrap();
        for index in 1..=4 {
            fs::write(
                memory_dir.join(format!("episodic/2026/04/16/process-{index}.md")),
                format!("# Agent Process Activity\nALAN_RECENT_RECALL_{index}\n"),
            )
            .unwrap();
        }

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text("What did we do yesterday?")]),
        )
        .expect("expected recall bundle");

        assert!(bundle.contains("## Runtime Recall Bundle"));
        assert!(bundle.contains(".alan/memory/daily/2026-04-16.md"));
        assert!(bundle.contains(".alan/memory/episodic/2026/04/16/process-4.md"));
        assert!(bundle.contains("ALAN_RECENT_RECALL_4"));
        assert!(!bundle.contains(".alan/memory/topics/recent-match-4.md"));
    }

    #[test]
    fn collect_markdown_files_recursive_prioritizes_newest_paths_under_cap() {
        let temp = TempDir::new().unwrap();
        let episodic_dir = temp.path().join(".alan/memory/episodic");
        fs::create_dir_all(episodic_dir.join("2026/04/15")).unwrap();
        fs::create_dir_all(episodic_dir.join("2026/04/16")).unwrap();
        fs::write(
            episodic_dir.join("2026/04/15/process-older.md"),
            "# Agent Process Activity\nolder\n",
        )
        .unwrap();
        fs::write(
            episodic_dir.join("2026/04/16/process-newer.md"),
            "# Agent Process Activity\nnewer\n",
        )
        .unwrap();

        let canonical_memory_root = fs::canonicalize(temp.path().join(".alan/memory")).unwrap();
        let collected = collect_markdown_files_recursive(&episodic_dir, &canonical_memory_root, 1);

        assert_eq!(
            collected,
            vec![episodic_dir.join("2026/04/16/process-newer.md")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn continuity_query_skips_symlinked_episodic_directories() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();
        fs::write(
            memory_dir.join("handoffs/LATEST.md"),
            "# Latest Handoff\nWe were refining the recall router.\n",
        )
        .unwrap();
        fs::create_dir_all(memory_dir.join("episodic/2026/04")).unwrap();

        let external_dir = temp.path().join("external-episodic");
        fs::create_dir_all(&external_dir).unwrap();
        fs::write(
            external_dir.join("process-leak.md"),
            "# Agent Process Activity\nZebraRecallLeak\n",
        )
        .unwrap();
        symlink(&external_dir, memory_dir.join("episodic/2026/04/link-out")).unwrap();

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text(
                "What were we doing in the previous Agent Process about ZebraRecallLeak?",
            )]),
        )
        .expect("expected recall bundle");

        assert!(!bundle.contains("ZebraRecallLeak"));
        assert!(!bundle.contains("process-leak.md"));
    }

    #[cfg(unix)]
    #[test]
    fn continuity_query_skips_handoff_paths_outside_memory_root() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join(".alan/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let external_handoffs = temp.path().join("external-handoffs");
        fs::create_dir_all(&external_handoffs).unwrap();
        fs::write(
            external_handoffs.join("LATEST.md"),
            "# Latest Handoff\nZebraHandoffLeak\n",
        )
        .unwrap();

        fs::remove_dir_all(memory_dir.join("handoffs")).unwrap();
        symlink(&external_handoffs, memory_dir.join("handoffs")).unwrap();

        let bundle = build_turn_recall_bundle(
            Some(&memory_dir),
            Some(&[crate::tape::ContentPart::text(
                "What were we doing in the previous Agent Process about ZebraHandoffLeak?",
            )]),
        );

        assert!(
            bundle.is_none()
                || bundle
                    .as_deref()
                    .is_some_and(|bundle| !bundle.contains("ZebraHandoffLeak"))
        );
    }
}
