use std::collections::HashSet;

use crate::{KeyKind, LanguageBackend, SearchConfig};
use rayon::prelude::*;

#[cfg(not(test))]
const PARALLEL_INDEX_THRESHOLD: usize = 50_000;
#[cfg(test)]
const PARALLEL_INDEX_THRESHOLD: usize = 4;

/// Byte span in the original candidate text that produced a generated key part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    /// Inclusive byte offset where the source span starts.
    pub start: usize,
    /// Exclusive byte offset where the source span ends.
    pub end: usize,
}

/// Indexed input row with display text and searchable keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// Stable input-order identifier.
    pub id: usize,
    /// Text shown to the user and emitted on selection.
    pub display: String,
    /// Searchable forms for this candidate.
    pub keys: Vec<SearchKey>,
}

/// One searchable form for a candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchKey {
    /// Text that the matcher scores against query variants.
    pub text: String,
    /// Semantic key type used for query compatibility checks.
    pub kind: KeyKind,
    /// Score adjustment for this key type.
    pub weight: i32,
    /// Optional map from key character positions back to original source spans.
    pub source_map: Option<Box<[Option<SourceSpan>]>>,
}

impl SearchKey {
    /// Attaches a source map to this key.
    pub fn with_source_map(mut self, source_map: Vec<Option<SourceSpan>>) -> Self {
        self.source_map = Some(source_map.into_boxed_slice());
        self
    }

    /// Creates an original-display search key.
    pub fn original(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::Original,
            weight: 3000,
            source_map: None,
        }
    }

    /// Creates a normalized-display search key.
    pub fn normalized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::Normalized,
            weight: 2800,
            source_map: None,
        }
    }

    /// Creates a Japanese kana-reading search key.
    pub fn kana_reading(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::KanaReading,
            weight: 1700,
            source_map: None,
        }
    }

    /// Creates a Japanese romaji-reading search key.
    pub fn romaji_reading(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::RomajiReading,
            weight: 1800,
            source_map: None,
        }
    }

    /// Creates a Chinese pinyin search key with separated syllables.
    pub fn pinyin_full(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinFull,
            weight: 1750,
            source_map: None,
        }
    }

    /// Creates a Chinese pinyin search key with syllables joined.
    pub fn pinyin_joined(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinJoined,
            weight: 1800,
            source_map: None,
        }
    }

    /// Creates a Chinese pinyin initials search key.
    pub fn pinyin_initials(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinInitials,
            weight: 1850,
            source_map: None,
        }
    }

    /// Creates a Korean romanized Hangul search key.
    pub fn korean_romanized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::KoreanRomanized,
            weight: 1800,
            source_map: None,
        }
    }

    /// Creates a Korean initial-consonant search key.
    pub fn korean_initials(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::KoreanInitials,
            weight: 1850,
            source_map: None,
        }
    }

    /// Creates a Korean keyboard-layout search key.
    pub fn korean_keyboard(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::KoreanKeyboard,
            weight: 1750,
            source_map: None,
        }
    }

    /// Creates a user-learned alias search key.
    pub fn learned_alias(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::LearnedAlias,
            weight: 2500,
            source_map: None,
        }
    }
}

/// Builds one indexed candidate using base and language-specific keys.
pub fn build_candidate(
    id: usize,
    display: impl Into<String>,
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Candidate {
    let display = display.into();
    let mut keys = vec![SearchKey::original(display.clone())];
    if config.normalize {
        keys.push(SearchKey::normalized(backend.normalize_candidate(&display)));
    }
    keys.extend(backend.build_candidate_keys(&display));
    let keys = dedup_and_limit_keys(keys, config);

    Candidate { id, display, keys }
}

/// Builds an index from input strings, using Rayon for large inputs.
pub fn build_index<I, S>(
    items: I,
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Vec<Candidate>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let items: Vec<_> = items.into_iter().map(Into::into).collect();
    if should_build_index_parallel(items.len()) {
        return items
            .into_par_iter()
            .enumerate()
            .map(|(id, item)| build_candidate(id, item, backend, config))
            .collect();
    }

    items
        .into_iter()
        .enumerate()
        .map(|(id, item)| build_candidate(id, item, backend, config))
        .collect()
}

fn should_build_index_parallel(len: usize) -> bool {
    len >= PARALLEL_INDEX_THRESHOLD && rayon::current_num_threads() > 1
}

/// Removes duplicate keys and caps generated key growth.
pub fn dedup_and_limit_keys(keys: Vec<SearchKey>, config: &SearchConfig) -> Vec<SearchKey> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut total_bytes = 0usize;

    for key in keys {
        if !seen.insert((key.kind, key.text.clone())) {
            continue;
        }

        let required_base_key = matches!(key.kind, KeyKind::Original | KeyKind::Normalized);
        let would_exceed_count = out.len() >= config.max_search_keys_per_candidate;
        let would_exceed_bytes =
            total_bytes + key.text.len() > config.max_total_key_bytes_per_candidate;

        if !required_base_key && (would_exceed_count || would_exceed_bytes) {
            continue;
        }

        total_bytes += key.text.len();
        out.push(key);
    }

    out
}

#[cfg(test)]
mod tests {
    use crate::{query::PlainBackend, KeyKind};

    use super::*;

    #[test]
    fn plain_mode_only_original_and_normalized() {
        let cand = build_candidate(0, "東京駅", &PlainBackend, &SearchConfig::default());

        assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
        assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Normalized));
        assert!(!cand
            .keys
            .iter()
            .any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading)));
    }

    #[test]
    fn original_key_is_always_present() {
        let cand = build_candidate(0, "README.md", &PlainBackend, &SearchConfig::default());
        assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
    }

    #[test]
    fn search_keys_are_deduped_and_capped() {
        let cfg = SearchConfig {
            max_search_keys_per_candidate: 4,
            ..SearchConfig::default()
        };
        let keys = vec![
            SearchKey::original("a"),
            SearchKey::normalized("a"),
            SearchKey::normalized("a"),
            SearchKey::learned_alias("b"),
            SearchKey::learned_alias("c"),
            SearchKey::learned_alias("d"),
        ];

        let out = dedup_and_limit_keys(keys, &cfg);

        assert!(out.len() <= 4);
        assert_eq!(
            out.len(),
            out.iter()
                .map(|k| (k.kind, k.text.as_str()))
                .collect::<HashSet<_>>()
                .len()
        );
    }

    #[test]
    fn parallel_index_preserves_input_order_and_ids() {
        let cfg = SearchConfig::default();
        let cand = build_index(["one", "two", "three", "four"], &PlainBackend, &cfg);

        assert_eq!(
            cand.iter()
                .map(|candidate| (candidate.id, candidate.display.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, "one"), (1, "two"), (2, "three"), (3, "four")]
        );
    }
}
