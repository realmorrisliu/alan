//! Pure-Rust BM25 + CJK unigram/bigram + English token recall engine.
//!
//! Indexes UTF-8 documents (including materialized [`crate::KnowledgeStore`]
//! roots) without external tokenizers or vector indexes, supporting pure
//! Chinese, mixed CJK+English, and English queries with deterministic recency
//! tie-breaking and bounded excerpts.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{ContentHash, KnowledgeError, KnowledgeStore};

const DEFAULT_BM25_K1: f64 = 1.5;
const DEFAULT_BM25_B: f64 = 0.75;
const DEFAULT_CJK_BIGRAM_WEIGHT: f64 = 2.0;
const DEFAULT_CJK_UNIGRAM_WEIGHT: f64 = 0.35;
const DEFAULT_SNIPPET_MAX_CHARS: usize = 1_200;
const SCORE_TIE_EPSILON: f64 = 1e-9;

/// Configuration parameters for the pure-Rust BM25 ranker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bm25Config {
    /// Term-frequency saturation factor (`k1`).
    pub k1: f64,
    /// Document-length normalization factor (`b` in `[0.0, 1.0]`).
    pub b: f64,
    /// Relative term weight for CJK character bigrams.
    pub cjk_bigram_weight: f64,
    /// Relative term weight for non-particle CJK unigrams.
    pub cjk_unigram_weight: f64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: DEFAULT_BM25_K1,
            b: DEFAULT_BM25_B,
            cjk_bigram_weight: DEFAULT_CJK_BIGRAM_WEIGHT,
            cjk_unigram_weight: DEFAULT_CJK_UNIGRAM_WEIGHT,
        }
    }
}

/// An indexed UTF-8 document stored in or projected from a knowledge store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedDoc {
    /// Logical identifier (for example a namespace path or bound root name).
    pub id: String,
    /// Optional content address of the backing checkpoint/block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<ContentHash>,
    /// Full UTF-8 document body.
    pub text: String,
    /// Monotonic recency rank (higher = newer) used to break equal BM25 scores.
    pub recency: u64,
}

impl IndexedDoc {
    /// Create an indexed document with default recency `0` and no content hash.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content_hash: None,
            text: text.into(),
            recency: 0,
        }
    }

    /// Attach a monotonic recency value (higher values sort first on score ties).
    pub fn with_recency(mut self, recency: u64) -> Self {
        self.recency = recency;
        self
    }

    /// Attach the backing [`ContentHash`] for provenance.
    pub fn with_content_hash(mut self, content_hash: ContentHash) -> Self {
        self.content_hash = Some(content_hash);
        self
    }
}

/// One ranked search hit returned by [`Ranker`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Document identifier.
    pub id: String,
    /// Backing checkpoint/content hash when indexed from [`KnowledgeStore`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<ContentHash>,
    /// BM25 relevance score.
    pub score: f64,
    /// Monotonic recency key carried from [`IndexedDoc`].
    pub recency: u64,
    /// Bounded match-oriented UTF-8 snippet (`<= max_snippet_chars`).
    pub snippet: String,
}

/// A weighted token emitted by [`tokenize_with_weights`].
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedToken {
    /// Normalized token text.
    pub term: String,
    /// Weight contribution (`1.0` for Latin words, higher for CJK bigrams).
    pub weight: f64,
}

#[derive(Debug, Clone)]
struct PostingDoc {
    doc: IndexedDoc,
    tf: HashMap<String, f64>,
    length: f64,
}

/// In-memory BM25 + CJK + English ranker over [`IndexedDoc`] entries.
#[derive(Debug, Clone)]
pub struct Ranker {
    config: Bm25Config,
    docs: Vec<PostingDoc>,
    doc_freq: HashMap<String, usize>,
    avg_doc_len: f64,
}

impl Ranker {
    /// Build a ranker with default BM25 parameters over `docs`.
    pub fn new(docs: Vec<IndexedDoc>) -> Self {
        Self::with_config(docs, Bm25Config::default())
    }

    /// Build a ranker with explicit BM25 parameters over `docs`.
    pub fn with_config(docs: Vec<IndexedDoc>, config: Bm25Config) -> Self {
        let mut postings = Vec::with_capacity(docs.len());
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let mut total_len = 0.0_f64;

        for doc in docs {
            let tokens = tokenize_with_weights(&doc.text, config);
            let mut tf: HashMap<String, f64> = HashMap::new();
            let mut length = 0.0_f64;
            let mut seen_terms = HashSet::new();

            for token in tokens {
                length += token.weight;
                *tf.entry(token.term.clone()).or_insert(0.0) += token.weight;
                seen_terms.insert(token.term);
            }
            for term in seen_terms {
                *doc_freq.entry(term).or_insert(0) += 1;
            }
            total_len += length;
            postings.push(PostingDoc { doc, tf, length });
        }

        let avg_doc_len = if postings.is_empty() {
            1.0
        } else {
            (total_len / postings.len() as f64).max(1.0)
        };

        Self {
            config,
            docs: postings,
            doc_freq,
            avg_doc_len,
        }
    }

    /// Index every UTF-8 document reachable from a live bound root in `store`.
    pub fn from_knowledge_store(store: &KnowledgeStore) -> Result<Self, KnowledgeError> {
        let mut docs = Vec::new();
        let mut bound_roots: Vec<_> = store.bound_roots().collect();
        bound_roots.sort_by(|left, right| left.name().cmp(right.name()));

        for (index, bound) in bound_roots.into_iter().enumerate() {
            let bytes = store.read_bound_root(bound)?;
            if let Ok(text) = String::from_utf8(bytes) {
                docs.push(
                    IndexedDoc::new(bound.name(), text)
                        .with_content_hash(bound.root_hash().clone())
                        .with_recency(index as u64 + 1),
                );
            }
        }
        Ok(Self::new(docs))
    }

    /// Search indexed documents using the default `1_200`-char snippet cap.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        self.search_with_snippet_limit(query, limit, DEFAULT_SNIPPET_MAX_CHARS)
    }

    /// Search indexed documents with an explicit snippet character cap.
    pub fn search_with_snippet_limit(
        &self,
        query: &str,
        limit: usize,
        max_snippet_chars: usize,
    ) -> Vec<SearchHit> {
        if limit == 0 || self.docs.is_empty() {
            return Vec::new();
        }

        let query_terms = aggregate_query_terms(query, self.config);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let doc_count = self.docs.len() as f64;
        let k1 = self.config.k1.max(0.0);
        let b = self.config.b.clamp(0.0, 1.0);

        let mut hits = Vec::new();
        for posting in &self.docs {
            let mut score = 0.0_f64;
            let norm = 1.0 - b + b * (posting.length / self.avg_doc_len);

            for (term, query_weight) in &query_terms {
                let Some(&tf) = posting.tf.get(term) else {
                    continue;
                };
                if tf <= 0.0 {
                    continue;
                }
                let df = *self.doc_freq.get(term).unwrap_or(&1) as f64;
                // Lucene smoothed non-negative IDF prevents 0/negative scores on small corpora.
                let idf = (1.0 + (doc_count - df + 0.5) / (df + 0.5)).ln();
                let denom = tf + k1 * norm;
                if denom > 0.0 {
                    score += query_weight * idf * (tf * (k1 + 1.0)) / denom;
                }
            }

            if score > 0.0 {
                hits.push(SearchHit {
                    id: posting.doc.id.clone(),
                    content_hash: posting.doc.content_hash.clone(),
                    score,
                    recency: posting.doc.recency,
                    snippet: excerpt_around_match(&posting.doc.text, query, max_snippet_chars),
                });
            }
        }

        hits.sort_by(|left, right| {
            let diff = right.score - left.score;
            if diff.abs() > SCORE_TIE_EPSILON {
                right.score.total_cmp(&left.score)
            } else {
                right
                    .recency
                    .cmp(&left.recency)
                    .then_with(|| left.id.cmp(&right.id))
            }
        });
        hits.truncate(limit);
        hits
    }

    /// Number of indexed documents.
    pub fn doc_count(&self) -> usize {
        self.docs.len()
    }
}

/// Format search hits as pretty JSON for file-server and CLI query surfaces.
pub fn format_hits_json(hits: &[SearchHit]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(hits)
}

/// Tokenize text into distinct normalized terms (deduplicated, preserving order).
pub fn tokenize_terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in tokenize_with_weights(text, Bm25Config::default()) {
        if seen.insert(token.term.clone()) {
            out.push(token.term);
        }
    }
    out
}

/// Tokenize UTF-8 text into weighted English/ASCII stems and CJK bigrams/unigrams.
pub fn tokenize_with_weights(text: &str, config: Bm25Config) -> Vec<WeightedToken> {
    let mut tokens = Vec::new();
    let mut latin_buf = String::new();
    let mut cjk_buf: Vec<char> = Vec::new();

    for ch in text.chars() {
        let normalized = normalize_fullwidth_ascii(ch);
        if is_cjk_char(normalized) {
            flush_latin(&mut latin_buf, &mut tokens);
            cjk_buf.push(normalized);
        } else if normalized.is_alphanumeric() {
            flush_cjk(&mut cjk_buf, config, &mut tokens);
            latin_buf.push(normalized);
        } else {
            flush_latin(&mut latin_buf, &mut tokens);
            flush_cjk(&mut cjk_buf, config, &mut tokens);
        }
    }
    flush_latin(&mut latin_buf, &mut tokens);
    flush_cjk(&mut cjk_buf, config, &mut tokens);
    tokens
}

/// Extract a bounded excerpt (`<= max_chars`) centered near the earliest query match.
pub fn excerpt_around_match(text: &str, query: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if max_chars == 0 {
        return String::new();
    }
    let total_chars = trimmed.chars().count();
    if total_chars <= max_chars {
        return trimmed.to_string();
    }

    let needles = raw_match_needles(query);
    let lower_trimmed = trimmed.to_lowercase();
    let match_char_offset = needles
        .iter()
        .filter_map(|needle| lower_trimmed.find(needle.as_str()))
        .min()
        .map(|byte_idx| lower_trimmed[..byte_idx].chars().count());

    let Some(match_pos) = match_char_offset else {
        return truncate_chars_with_ellipsis(trimmed, max_chars);
    };

    // Keep the beginning of the file if the first match already fits inside the leading window.
    if match_pos < max_chars.saturating_sub(80) {
        return truncate_chars_with_ellipsis(trimmed, max_chars);
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let window_budget = max_chars.saturating_sub(6); // space for "... " and "..."
    let half = window_budget / 3;
    let start = match_pos.saturating_sub(half);
    let end = (start + window_budget).min(chars.len());
    let adjusted_start = end.saturating_sub(window_budget);

    let mut out = String::new();
    if adjusted_start > 0 {
        out.push_str("...");
    }
    out.extend(chars[adjusted_start..end].iter());
    if end < chars.len() {
        out.push_str("...");
    }
    out
}

fn aggregate_query_terms(query: &str, config: Bm25Config) -> BTreeMap<String, f64> {
    let mut terms = BTreeMap::new();
    for token in tokenize_with_weights(query, config) {
        let entry = terms.entry(token.term).or_insert(0.0_f64);
        if token.weight > *entry {
            *entry = token.weight;
        }
    }
    terms
}

fn flush_latin(buf: &mut String, out: &mut Vec<WeightedToken>) {
    if buf.is_empty() {
        return;
    }
    let raw = std::mem::take(buf);
    if let Some(stem) = normalize_latin_token(&raw) {
        out.push(WeightedToken {
            term: stem,
            weight: 1.0,
        });
    }
}

fn flush_cjk(buf: &mut Vec<char>, config: Bm25Config, out: &mut Vec<WeightedToken>) {
    if buf.is_empty() {
        return;
    }
    let chars = std::mem::take(buf);

    for pair in chars.windows(2) {
        let (c1, c2) = (pair[0], pair[1]);
        if is_cjk_noise_bigram(c1, c2, chars.len()) {
            continue;
        }
        let mut bigram = String::with_capacity(c1.len_utf8() + c2.len_utf8());
        bigram.push(c1);
        bigram.push(c2);
        out.push(WeightedToken {
            term: bigram,
            weight: config.cjk_bigram_weight,
        });
    }

    for ch in chars {
        if is_cjk_stop_particle(ch) {
            continue;
        }
        out.push(WeightedToken {
            term: ch.to_string(),
            weight: config.cjk_unigram_weight,
        });
    }
}

fn normalize_latin_token(raw: &str) -> Option<String> {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() || is_english_stop_word(&lower) {
        return None;
    }
    if lower.len() == 1 && lower.chars().all(|ch| ch.is_ascii_alphabetic()) {
        // Keep single technical letters like `q` or `c` only when not stop words (`a`, `i` filtered above).
        if matches!(
            lower.as_str(),
            "a" | "i" | "s" | "t" | "m" | "d" | "o" | "u" | "y"
        ) {
            return None;
        }
    }

    let stemmed = stem_english_ascii(&lower);
    if stemmed.is_empty() || is_english_stop_word(&stemmed) {
        return None;
    }
    Some(stemmed)
}

fn stem_english_ascii(word: &str) -> String {
    if !word.is_ascii() || word.len() < 4 || !word.bytes().all(|b| b.is_ascii_alphabetic()) {
        return word.to_string();
    }

    if let Some(stem) = word.strip_suffix("sses")
        && stem.len() >= 2
    {
        return format!("{stem}ss");
    }
    if let Some(stem) = word.strip_suffix("ies")
        && stem.len() >= 2
    {
        return format!("{stem}y");
    }
    if (word.ends_with("ches")
        || word.ends_with("shes")
        || word.ends_with("xes")
        || word.ends_with("zes"))
        && word.len() > 4
    {
        // Words like "caches" / "aches" add -s to -che, while "matches"/"branches"/"fixes" add -es.
        if word.ends_with("aches") {
            return word[..word.len() - 1].to_string();
        }
        return word[..word.len() - 2].to_string();
    }
    if let Some(stem) = word.strip_suffix("ing")
        && stem.len() >= 3
        && has_ascii_vowel(stem)
    {
        return collapse_double_consonant(stem);
    }
    if let Some(stem) = word.strip_suffix("ed")
        && stem.len() >= 3
        && has_ascii_vowel(stem)
    {
        return collapse_double_consonant(stem);
    }
    if let Some(stem) = word.strip_suffix("ly")
        && stem.len() >= 4
        && has_ascii_vowel(stem)
    {
        return stem.to_string();
    }
    if word.ends_with('s')
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
        && word.len() > 3
    {
        return word[..word.len() - 1].to_string();
    }

    word.to_string()
}

fn collapse_double_consonant(stem: &str) -> String {
    let bytes = stem.as_bytes();
    if bytes.len() >= 3 {
        let last = bytes[bytes.len() - 1];
        let prev = bytes[bytes.len() - 2];
        if last == prev && matches!(last, b'b' | b'd' | b'g' | b'm' | b'n' | b'p' | b'r' | b't') {
            return stem[..stem.len() - 1].to_string();
        }
    }
    stem.to_string()
}

fn has_ascii_vowel(s: &str) -> bool {
    s.bytes()
        .any(|b| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y'))
}

fn is_english_stop_word(word: &str) -> bool {
    matches!(
        word,
        "a" | "about"
            | "after"
            | "again"
            | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "because"
            | "been"
            | "before"
            | "being"
            | "but"
            | "by"
            | "can"
            | "could"
            | "did"
            | "do"
            | "does"
            | "doing"
            | "done"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "having"
            | "he"
            | "her"
            | "hers"
            | "him"
            | "his"
            | "how"
            | "if"
            | "in"
            | "into"
            | "is"
            | "it"
            | "its"
            | "just"
            | "me"
            | "more"
            | "most"
            | "my"
            | "no"
            | "not"
            | "of"
            | "on"
            | "or"
            | "our"
            | "ours"
            | "out"
            | "over"
            | "she"
            | "should"
            | "so"
            | "some"
            | "such"
            | "than"
            | "that"
            | "the"
            | "their"
            | "theirs"
            | "them"
            | "then"
            | "there"
            | "these"
            | "they"
            | "this"
            | "those"
            | "through"
            | "to"
            | "too"
            | "under"
            | "up"
            | "us"
            | "very"
            | "was"
            | "we"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "who"
            | "whom"
            | "why"
            | "will"
            | "with"
            | "would"
            | "you"
            | "your"
            | "yours"
    )
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{3040}'..='\u{309F}' // Hiragana
            | '\u{30A0}'..='\u{30FF}' // Katakana
            | '\u{3100}'..='\u{312F}' // Bopomofo
            | '\u{3130}'..='\u{318F}' // Hangul Compatibility Jamo
            | '\u{3400}'..='\u{4DBF}' // CJK Extension A
            | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
            | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
            | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
            | '\u{20000}'..='\u{2FA1F}' // CJK Extensions B-F
            | '\u{30000}'..='\u{323AF}' // CJK Extensions G-I
    )
}

fn normalize_fullwidth_ascii(ch: char) -> char {
    match ch {
        '\u{FF10}'..='\u{FF19}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
        '\u{FF21}'..='\u{FF3A}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
        '\u{FF41}'..='\u{FF5A}' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
        _ => ch,
    }
}

fn is_cjk_stop_particle(ch: char) -> bool {
    matches!(
        ch,
        '的' | '了'
            | '着'
            | '过'
            | '吗'
            | '呢'
            | '吧'
            | '啊'
            | '呀'
            | '哦'
            | '嘛'
            | '么'
            | '之'
            | '乎'
            | '者'
            | '也'
            | '是'
            | '有'
            | '在'
            | '和'
            | '与'
            | '或'
            | '及'
            | '把'
            | '被'
            | '给'
            | '让'
            | '这'
            | '那'
            | '哪'
            | '谁'
            | '什'
            | '几'
            | '种'
            | '里'
            | '很'
            | '太'
            | '都'
            | '就'
            | '才'
    )
}

fn is_cjk_noise_bigram(c1: char, c2: char, run_len: usize) -> bool {
    if is_cjk_stop_particle(c1) && is_cjk_stop_particle(c2) {
        return true;
    }
    if run_len > 2
        && matches!(c2, '的' | '了' | '吗' | '呢' | '吧' | '啊' | '呀')
        && matches!(
            c1,
            '我' | '你'
                | '他'
                | '她'
                | '它'
                | '这'
                | '那'
                | '哪'
                | '里'
                | '中'
                | '上'
                | '下'
                | '用'
                | '是'
                | '有'
                | '做'
        )
    {
        return true;
    }
    if run_len > 2 && matches!(c1, '的' | '了' | '吗' | '呢' | '吧' | '啊' | '哪') {
        return true;
    }
    false
}

fn raw_match_needles(query: &str) -> Vec<String> {
    let mut needles = Vec::new();
    let mut latin = String::new();
    let mut cjk: Vec<char> = Vec::new();

    for ch in query.chars() {
        let normalized = normalize_fullwidth_ascii(ch);
        if is_cjk_char(normalized) {
            if latin.len() >= 2 && !is_english_stop_word(&latin.to_lowercase()) {
                needles.push(latin.to_lowercase());
            }
            latin.clear();
            cjk.push(normalized);
        } else if normalized.is_alphanumeric() {
            push_cjk_needles(&mut cjk, &mut needles);
            latin.push(normalized);
        } else {
            if latin.len() >= 2 && !is_english_stop_word(&latin.to_lowercase()) {
                needles.push(latin.to_lowercase());
            }
            latin.clear();
            push_cjk_needles(&mut cjk, &mut needles);
        }
    }
    if latin.len() >= 2 && !is_english_stop_word(&latin.to_lowercase()) {
        needles.push(latin.to_lowercase());
    }
    push_cjk_needles(&mut cjk, &mut needles);
    needles
}

fn push_cjk_needles(cjk: &mut Vec<char>, needles: &mut Vec<String>) {
    if cjk.is_empty() {
        return;
    }
    let chars = std::mem::take(cjk);
    for pair in chars.windows(2) {
        if !is_cjk_noise_bigram(pair[0], pair[1], chars.len()) {
            let mut bigram = String::new();
            bigram.push(pair[0]);
            bigram.push(pair[1]);
            needles.push(bigram);
        }
    }
    if needles.is_empty() {
        for ch in chars {
            if !is_cjk_stop_particle(ch) {
                needles.push(ch.to_string());
            }
        }
    }
}

fn truncate_chars_with_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RootAccess;

    #[test]
    fn pure_chinese_and_mixed_cjk_english_queries_rank_best_match_first() {
        let ranker = Ranker::new(vec![
            IndexedDoc::new("other.md", "今天讨论了前端样式和发布清单。").with_recency(10),
            IndexedDoc::new(
                "prefs.md",
                "用户偏好：回答时优先使用中文，技术栈偏好 Rust。",
            )
            .with_recency(1),
            IndexedDoc::new(
                "cargo.md",
                "排查 Rust 项目里的 Cargo 报错：运行 cargo check --workspace 修复 feature 冲突。",
            )
            .with_recency(2),
        ]);

        let zh_hits = ranker.search("用户偏好用哪种语言？", 3);
        assert!(!zh_hits.is_empty());
        assert_eq!(zh_hits[0].id, "prefs.md");
        assert!(zh_hits[0].score > 0.0);

        let mixed_hits = ranker.search("Rust 项目里的 Cargo 报错", 3);
        assert!(!mixed_hits.is_empty());
        assert_eq!(mixed_hits[0].id, "cargo.md");
    }

    #[test]
    fn english_stemming_and_idf_favor_rare_matching_terms() {
        let ranker = Ranker::new(vec![
            IndexedDoc::new("common.md", "runtime config notes and runtime flags").with_recency(5),
            IndexedDoc::new(
                "rare.md",
                "runtime queries failed due to stale checkpoint indexes",
            )
            .with_recency(1),
        ]);

        let hits = ranker.search("query checkpoint failure", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "rare.md");
    }

    #[test]
    fn tied_scores_break_by_recency_descending() {
        let ranker = Ranker::new(vec![
            IndexedDoc::new("2026-04-01.md", "AlphaBetaMarker note").with_recency(100),
            IndexedDoc::new("2026-04-02.md", "AlphaBetaMarker note").with_recency(200),
        ]);

        let hits = ranker.search("AlphaBetaMarker", 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "2026-04-02.md");
        assert_eq!(hits[1].id, "2026-04-01.md");
    }

    #[test]
    fn ranker_indexes_bound_roots_from_knowledge_store() {
        let mut store = KnowledgeStore::new();
        let root_a = store
            .checkpoint_from_bytes([b"Rust Cargo build error E0433".as_slice()])
            .unwrap();
        let root_b = store
            .checkpoint_from_bytes(["用户偏好使用中文和 Rust".as_bytes()])
            .unwrap();
        store
            .bind_root("mem/cargo.md", root_a.clone(), RootAccess::ReadOnly)
            .unwrap();
        store
            .bind_root("mem/user.md", root_b, RootAccess::ReadOnly)
            .unwrap();

        let ranker = Ranker::from_knowledge_store(&store).unwrap();
        assert_eq!(ranker.doc_count(), 2);
        let hits = ranker.search("Cargo E0433", 2);
        assert_eq!(hits[0].id, "mem/cargo.md");
        assert_eq!(hits[0].content_hash, Some(root_a));
    }

    #[test]
    fn excerpt_around_match_respects_char_limit_and_centers_late_matches() {
        let long_doc = format!(
            "{}ImportantTargetKeyword appears near the end.",
            "filler text ".repeat(200)
        );
        let snippet = excerpt_around_match(&long_doc, "ImportantTargetKeyword", 120);
        assert!(snippet.chars().count() <= 120);
        assert!(snippet.contains("ImportantTargetKeyword"));
    }
}
