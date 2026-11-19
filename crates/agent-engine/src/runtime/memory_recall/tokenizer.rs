//! Pure-Rust tokenization, stemming, CJK bigram/unigram splitting, and bounded excerpting.

use std::collections::BTreeMap;

const CJK_BIGRAM_WEIGHT: f64 = 2.0;
const CJK_UNIGRAM_WEIGHT: f64 = 0.35;

pub(super) fn aggregate_query_terms(query: &str) -> BTreeMap<String, f64> {
    let mut terms = BTreeMap::new();
    for (term, weight) in tokenize_with_weights(query) {
        let entry = terms.entry(term).or_insert(0.0_f64);
        if weight > *entry {
            *entry = weight;
        }
    }
    terms
}

pub(super) fn tokenize_with_weights(text: &str) -> Vec<(String, f64)> {
    let mut tokens = Vec::new();
    let mut latin_buf = String::new();
    let mut cjk_buf: Vec<char> = Vec::new();

    for ch in text.chars() {
        let normalized = normalize_fullwidth_ascii(ch);
        if is_cjk_char(normalized) {
            flush_latin(&mut latin_buf, &mut tokens);
            cjk_buf.push(normalized);
        } else if normalized.is_alphanumeric() {
            flush_cjk(&mut cjk_buf, &mut tokens);
            latin_buf.push(normalized);
        } else {
            flush_latin(&mut latin_buf, &mut tokens);
            flush_cjk(&mut cjk_buf, &mut tokens);
        }
    }
    flush_latin(&mut latin_buf, &mut tokens);
    flush_cjk(&mut cjk_buf, &mut tokens);
    tokens
}

pub(super) fn excerpt_around_match(text: &str, query: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if max_chars == 0 {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let needles = raw_match_needles(query);
    let lower_trimmed = trimmed.to_lowercase();
    let match_pos = needles
        .iter()
        .filter_map(|needle| lower_trimmed.find(needle.as_str()))
        .min()
        .map(|byte_idx| lower_trimmed[..byte_idx].chars().count());

    let Some(match_pos) = match_pos else {
        return truncate_chars(trimmed, max_chars);
    };
    if match_pos < max_chars.saturating_sub(80) {
        return truncate_chars(trimmed, max_chars);
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let window_budget = max_chars.saturating_sub(6);
    let start = match_pos.saturating_sub(window_budget / 3);
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

fn flush_latin(buf: &mut String, out: &mut Vec<(String, f64)>) {
    if buf.is_empty() {
        return;
    }
    let raw = std::mem::take(buf);
    if let Some(stem) = normalize_latin_token(&raw) {
        out.push((stem, 1.0));
    }
}

fn flush_cjk(buf: &mut Vec<char>, out: &mut Vec<(String, f64)>) {
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
        out.push((bigram, CJK_BIGRAM_WEIGHT));
    }

    for ch in chars {
        if is_cjk_stop_particle(ch) {
            continue;
        }
        out.push((ch.to_string(), CJK_UNIGRAM_WEIGHT));
    }
}

fn normalize_latin_token(raw: &str) -> Option<String> {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() || is_english_stop_word(&lower) {
        return None;
    }
    if lower.len() == 1
        && lower.chars().all(|ch| ch.is_ascii_alphabetic())
        && matches!(
            lower.as_str(),
            "a" | "i" | "s" | "t" | "m" | "d" | "o" | "u" | "y"
        )
    {
        return None;
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
        '\u{3040}'..='\u{309F}'
            | '\u{30A0}'..='\u{30FF}'
            | '\u{3100}'..='\u{312F}'
            | '\u{3130}'..='\u{318F}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{20000}'..='\u{2FA1F}'
            | '\u{30000}'..='\u{323AF}'
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
