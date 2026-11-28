//! Subsequence and fuzzy matching for node search and library panels.

use std::ops::Range;
use unicode_normalization::UnicodeNormalization;

/// Caps normalized candidate and query text so oversized inputs cannot cause
/// unbounded allocations during per-frame UI filtering.
const MAX_SEARCH_BYTES: usize = 4096;

/// How a candidate field participates in matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Full fuzzy subsequence matching plus exact/prefix/substring bonuses (for display labels and slugs).
    Fuzzy,
    /// Normalized contiguous substring matching only (prevents scattered hits across URIs and categories).
    Substring,
}

/// Candidate field descriptor used by [`FuzzyMatcher::score_fields`] and [`FuzzyMatcher::matches_fields`].
#[derive(Debug, Clone, Copy)]
pub struct SearchField<'a> {
    pub text: &'a str,
    pub offset: i32,
    pub mode: MatchMode,
}

impl<'a> SearchField<'a> {
    /// Creates a fuzzy-searchable field with no score offset.
    #[must_use]
    pub const fn fuzzy(text: &'a str) -> Self {
        Self { text, offset: 0, mode: MatchMode::Fuzzy }
    }

    /// Creates a substring-only field with no score offset.
    #[must_use]
    pub const fn substring(text: &'a str) -> Self {
        Self { text, offset: 0, mode: MatchMode::Substring }
    }

    /// Creates a substring-only field with a score offset (e.g. a penalty for category matches).
    #[must_use]
    pub const fn substring_with_offset(text: &'a str, offset: i32) -> Self {
        Self { text, offset, mode: MatchMode::Substring }
    }
}

/// Case-insensitive, Unicode-normalized fuzzy subsequence score (higher is better).
/// Returns `None` if not a match.
///
/// For matching many candidates against the same query inside a UI loop, prefer [`FuzzyMatcher`].
///
/// Matching rules:
/// 1. An empty query matches everything with score 0.
/// 2. Every non-whitespace character in `query` must appear in `hay` in sequential order.
/// 3. Scoring rewards:
///    - Exact match bonus (+100)
///    - Exact prefix match bonus (+50)
///    - Word prefix match bonus (+40)
///    - Exact substring containment bonus (+30)
///    - Contiguous character matches (+8 per character)
///    - Word-boundary matches: start of string or right after `' '`, `'_'`, `'-'`, `'/'` (+6 per character)
/// 4. Length penalty: subtracts `hay.len() / 16` so shorter matches rank ahead of longer names.
///
/// Text is normalized to Unicode NFC after lowercasing using `unicode-normalization` (UAX #15).
pub fn fuzzy_score(hay: &str, query: &str) -> Option<i32> {
    FuzzyMatcher::new(query).score(hay)
}

/// Pre-normalized query and reusable scratch buffers for candidate matching across a UI pass.
///
/// Normalizes the query once on construction and reuses internal scratch storage across
/// candidates so scoring or filtering full catalogs performs minimal allocations.
#[derive(Debug, Clone, Default)]
pub struct FuzzyMatcher {
    norm_query: String,
    word_ranges: Vec<Range<usize>>,
    scratch: String,
    field_bufs: Vec<String>,
}

impl FuzzyMatcher {
    /// Pre-normalizes `query` once and indexes its whitespace-delimited search tokens.
    #[must_use]
    pub fn new(query: &str) -> Self {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Self::default();
        }

        let mut norm_query = String::new();
        normalize_into(trimmed, &mut norm_query);

        let mut word_ranges = Vec::new();
        let mut word_start: Option<usize> = None;
        for (idx, ch) in norm_query.char_indices() {
            if ch.is_whitespace() {
                if let Some(start) = word_start.take() {
                    word_ranges.push(start..idx);
                }
            } else if word_start.is_none() {
                word_start = Some(idx);
            }
        }
        if let Some(start) = word_start {
            word_ranges.push(start..norm_query.len());
        }

        Self { norm_query, word_ranges, scratch: String::new(), field_bufs: Vec::new() }
    }

    /// Returns `true` when the query contains no non-whitespace characters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.word_ranges.is_empty()
    }

    /// Scores a single candidate string against the prepared query.
    pub fn score(&mut self, hay: &str) -> Option<i32> {
        if self.word_ranges.is_empty() {
            return Some(0);
        }
        let mut buf = std::mem::take(&mut self.scratch);
        normalize_into(hay, &mut buf);
        let result = self.score_norm_buf(&buf, MatchMode::Fuzzy);
        self.scratch = buf;
        result
    }

    /// Scores a candidate across multiple configured [`SearchField`]s.
    ///
    /// Matches either any single field against all query words, or multi-word queries split across
    /// distinct fields (choosing the highest-scoring field per word).
    pub fn score_fields(&mut self, fields: &[SearchField<'_>]) -> Option<i32> {
        if self.word_ranges.is_empty() {
            return Some(0);
        }

        if let [only] = self.word_ranges.as_slice() {
            let word = self.norm_query.get(only.clone())?;
            let mut best: Option<i32> = None;
            for field in fields {
                normalize_into(field.text, &mut self.scratch);
                if let Some(s) = single_word_score(&self.scratch, word, field.mode) {
                    let weighted = s.saturating_add(field.offset);
                    best = Some(best.map_or(weighted, |cur| cur.max(weighted)));
                }
            }
            return best;
        }

        self.normalize_fields(fields.iter().map(|field| field.text));
        let count = fields.len();

        let whole_score = self
            .field_bufs
            .iter()
            .take(count)
            .zip(fields.iter())
            .filter_map(|(buf, field)| self.score_norm_buf(buf, field.mode).map(|s| s.saturating_add(field.offset)))
            .max();

        let cross_score = self.word_ranges.iter().try_fold(0i32, |total, range| {
            let word = self.norm_query.get(range.clone())?;
            let best_word = self
                .field_bufs
                .iter()
                .take(count)
                .zip(fields.iter())
                .filter_map(|(buf, field)| single_word_score(buf, word, field.mode).map(|s| s.saturating_add(field.offset)))
                .max()?;
            Some(total.saturating_add(best_word))
        });

        whole_score.max(cross_score)
    }

    /// Returns `true` when every query word matches at least one configured [`SearchField`].
    pub fn matches_fields(&mut self, fields: &[SearchField<'_>]) -> bool {
        if self.word_ranges.is_empty() {
            return true;
        }

        if let [only] = self.word_ranges.as_slice() {
            let Some(word) = self.norm_query.get(only.clone()) else {
                return false;
            };
            return fields.iter().any(|field| {
                normalize_into(field.text, &mut self.scratch);
                single_word_score(&self.scratch, word, field.mode).is_some()
            });
        }

        self.normalize_fields(fields.iter().map(|field| field.text));
        let count = fields.len();
        self.word_ranges.iter().all(|range| {
            let Some(word) = self.norm_query.get(range.clone()) else {
                return false;
            };
            self.field_bufs.iter().take(count).zip(fields.iter()).any(|(buf, field)| single_word_score(buf, word, field.mode).is_some())
        })
    }

    fn normalize_fields<'a>(&mut self, texts: impl ExactSizeIterator<Item = &'a str>) {
        let needed = texts.len();
        if self.field_bufs.len() < needed {
            self.field_bufs.resize_with(needed, String::new);
        }
        for (buf, text) in self.field_bufs.iter_mut().zip(texts) {
            normalize_into(text, buf);
        }
    }

    fn score_norm_buf(&self, hay_norm: &str, mode: MatchMode) -> Option<i32> {
        self.word_ranges.iter().try_fold(0i32, |total, range| {
            let word = self.norm_query.get(range.clone())?;
            let word_score = single_word_score(hay_norm, word, mode)?;
            Some(total.saturating_add(word_score))
        })
    }
}

/// Truncates `s` to at most `max_bytes` on a valid UTF-8 scalar boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or("")
}

/// Lowercases and normalizes `src` into `out` (ASCII fast path preserves identical NFC semantics).
fn normalize_into(src: &str, out: &mut String) {
    out.clear();
    let bounded = truncate_utf8(src, MAX_SEARCH_BYTES);
    if bounded.is_ascii() {
        out.push_str(bounded);
        out.make_ascii_lowercase();
    } else {
        out.extend(bounded.chars().flat_map(char::to_lowercase).nfc());
    }
}

/// Scores a single normalized, whitespace-free query word against normalized candidate text.
///
/// Returns `None` if `mode` requirements are not satisfied.
fn single_word_score(hay_norm: &str, q_norm: &str, mode: MatchMode) -> Option<i32> {
    // Equal Unicode scalars have identical UTF-8 byte lengths, so any matching
    // subsequence requires at least `q_norm.len()` bytes in `hay_norm`.
    if hay_norm.len() < q_norm.len() {
        return None;
    }

    let mut score = 0i32;

    if hay_norm == q_norm {
        score = score.saturating_add(100);
    } else if hay_norm.starts_with(q_norm) {
        score = score.saturating_add(50);
    } else if hay_norm.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q_norm)) {
        score = score.saturating_add(40);
    } else if hay_norm.contains(q_norm) {
        score = score.saturating_add(30);
    } else if mode == MatchMode::Substring {
        return None;
    }

    let mut last_end_idx: Option<usize> = None;
    let mut chars = hay_norm.char_indices();
    for q in q_norm.chars() {
        let (idx, ch) = chars.by_ref().find(|(_, c)| *c == q)?;
        let contiguous = match last_end_idx {
            Some(prev_end) if idx == prev_end => 8,
            _ => 1,
        };
        score = score.saturating_add(contiguous);

        let is_boundary = idx == 0 || matches!(hay_norm.as_bytes().get(idx.saturating_sub(1)), Some(&b' ' | &b'_' | &b'-' | &b'/'));
        if is_boundary {
            score = score.saturating_add(6);
        }
        last_end_idx = Some(idx.saturating_add(ch.len_utf8()));
    }

    let len_penalty = i32::try_from(hay_norm.len() / 16).unwrap_or(i32::MAX);
    Some(score.saturating_sub(len_penalty))
}

#[cfg(test)]
mod tests {
    use super::{FuzzyMatcher, MAX_SEARCH_BYTES, SearchField, fuzzy_score};

    #[test]
    fn test_empty_query_matches_with_zero() {
        assert_eq!(fuzzy_score("Gaussian Blur", ""), Some(0));
        assert_eq!(fuzzy_score("Gaussian Blur", "   "), Some(0));
    }

    #[test]
    fn test_exact_match_score() {
        let score = fuzzy_score("blur", "blur").unwrap();
        assert!(score >= 100);
    }

    #[test]
    fn test_case_insensitive_matching() {
        assert!(fuzzy_score("Gaussian Blur", "gblur").is_some());
        assert!(fuzzy_score("Gaussian Blur", "GBLUR").is_some());
    }

    #[test]
    fn test_subsequence_non_match() {
        assert_eq!(fuzzy_score("Gaussian Blur", "xyz"), None);
        assert_eq!(fuzzy_score("Blur", "blura"), None);
    }

    #[test]
    fn test_shorter_matches_rank_higher() {
        let short_score = fuzzy_score("Blur", "blur").unwrap();
        let long_score = fuzzy_score("Directional Blur Advanced", "blur").unwrap();
        assert!(short_score > long_score);
    }

    #[test]
    fn test_multi_word_query() {
        assert!(fuzzy_score("Directional Blur", "dir blur").is_some());
        assert!(fuzzy_score("Directional Blur", "dir\u{00a0}blur").is_some());
        assert_eq!(fuzzy_score("Directional Blur", "dir xyz"), None);
    }

    #[test]
    fn test_combining_diacritics_normalization() {
        assert_eq!(fuzzy_score("Crème Brûlée", "crème brûlée"), fuzzy_score("crème brûlée", "crème brûlée"));
        assert!(fuzzy_score("nai\u{0308}ve", "naïve").is_some());
        assert!(fuzzy_score("naïve", "nai\u{0308}ve").is_some());
        assert!(fuzzy_score("e\u{0301}clat", "é").is_some());
        assert!(fuzzy_score("éclat", "e\u{0301}").is_some());
        assert!(fuzzy_score("Å", "A\u{030a}").is_some());
        assert!(fuzzy_score("č", "c\u{030c}").is_some());
        assert_eq!(fuzzy_score("plain", "p\u{0315}lain"), None);
    }

    #[test]
    fn test_reordered_combining_marks_match() {
        // c + acute + cedilla vs c + cedilla + acute
        let s1 = "c\u{0301}\u{0327}at";
        let s2 = "c\u{0327}\u{0301}at";
        assert_eq!(fuzzy_score(s1, s2), fuzzy_score(s1, s1));
        assert!(fuzzy_score(s1, s2).is_some());
    }

    #[test]
    fn test_multi_accent_canonical_equivalence() {
        // Vietnamese ắ: precomposed (U+1EAF) vs decomposed (a + U+0306 breve + U+0301 acute)
        let precomposed = "m\u{1eaf}t";
        let decomposed = "ma\u{0306}\u{0301}t";
        assert_eq!(fuzzy_score(precomposed, decomposed), fuzzy_score(precomposed, precomposed));
        assert_eq!(fuzzy_score(decomposed, precomposed), fuzzy_score(precomposed, precomposed));
        assert!(fuzzy_score(precomposed, decomposed).is_some());
    }

    #[test]
    fn test_additional_canonical_forms_dot_and_j_caron() {
        // Latin g with dot above, dotted capital I + macron, and j with caron
        assert_eq!(fuzzy_score("\u{0121}ap", "g\u{0307}ap"), fuzzy_score("\u{0121}ap", "\u{0121}ap"));
        assert_eq!(fuzzy_score("\u{0130}\u{0304}", "i\u{0307}\u{0304}"), fuzzy_score("\u{0130}\u{0304}", "\u{0130}\u{0304}"));
        assert_eq!(fuzzy_score("\u{01f0}", "j\u{030c}"), fuzzy_score("\u{01f0}", "\u{01f0}"));
    }

    #[test]
    fn test_l_dot_above_distinct_from_middle_dot() {
        // l + U+0307 (combining dot above) must NOT be conflated with U+0140 (ŀ, L with middle dot)
        let l_combining_dot = "l\u{0307}abel";
        let l_middle_dot = "\u{0140}abel";
        assert_eq!(fuzzy_score(l_combining_dot, l_middle_dot), None);
    }

    #[test]
    fn test_multibyte_character_contiguity_tracking() {
        let score = fuzzy_score("🦀🦀🦀", "🦀🦀").unwrap();
        assert!(score > 0);
    }

    #[test]
    fn test_matcher_reuse_across_fields_and_candidates() {
        let mut matcher = FuzzyMatcher::new("filters blur");
        assert!(!matcher.is_empty());

        let fields = [SearchField::fuzzy("Blur"), SearchField::substring("node:blur"), SearchField::substring_with_offset("Filters/Blurs", -20)];
        assert!(matcher.score_fields(&fields).is_some());
        assert!(matcher.matches_fields(&fields));

        // Reuse with fewer fields must ignore stale buffer slots.
        let other = [SearchField::fuzzy("Noise"), SearchField::substring("node:perlin")];
        assert!(!matcher.matches_fields(&other));
        assert_eq!(matcher.score_fields(&other), None);

        // Substring mode must reject scattered subsequence matches across URI ids.
        let mut uri_noise = FuzzyMatcher::new("blur");
        assert!(!uri_noise.matches_fields(&[
            SearchField::fuzzy("ACEScg to linear sRGB"),
            SearchField::substring("sbs://library.sbs#Functions/Color/acescg_to_srgb"),
            SearchField::substring("Functions/Color"),
        ]));
    }

    #[test]
    fn test_oversized_inputs_remain_bounded() {
        let huge = "a".repeat(MAX_SEARCH_BYTES * 4);
        assert!(fuzzy_score(&huge, "aaa").is_some());
        let huge_mb = "é".repeat(MAX_SEARCH_BYTES);
        assert!(fuzzy_score(&huge_mb, "e\u{0301}").is_some());
    }
}
