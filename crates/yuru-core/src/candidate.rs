use std::collections::HashSet;

use crate::{KeyKind, LanguageBackend, SearchConfig};
use rayon::prelude::*;

#[cfg(not(test))]
const PARALLEL_INDEX_THRESHOLD: usize = 50_000;
#[cfg(test)]
const PARALLEL_INDEX_THRESHOLD: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    pub id: usize,
    pub display: String,
    pub keys: Vec<SearchKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchKey {
    pub text: String,
    pub kind: KeyKind,
    pub weight: i32,
    pub source_map: Option<Box<[Option<SourceSpan>]>>,
}

impl SearchKey {
    pub fn with_source_map(mut self, source_map: Vec<Option<SourceSpan>>) -> Self {
        self.source_map = Some(source_map.into_boxed_slice());
        self
    }

    pub fn original(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::Original,
            weight: 3000,
            source_map: None,
        }
    }

    pub fn normalized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::Normalized,
            weight: 2800,
            source_map: None,
        }
    }

    pub fn kana_reading(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::KanaReading,
            weight: 1700,
            source_map: None,
        }
    }

    pub fn romaji_reading(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::RomajiReading,
            weight: 1800,
            source_map: None,
        }
    }

    pub fn pinyin_full(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinFull,
            weight: 1750,
            source_map: None,
        }
    }

    pub fn pinyin_joined(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinJoined,
            weight: 1800,
            source_map: None,
        }
    }

    pub fn pinyin_initials(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::PinyinInitials,
            weight: 1850,
            source_map: None,
        }
    }

    pub fn learned_alias(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: KeyKind::LearnedAlias,
            weight: 2500,
            source_map: None,
        }
    }
}

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
