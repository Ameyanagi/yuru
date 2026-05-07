use std::collections::HashSet;

use crate::{KeyKind, LanguageBackend, SearchConfig};
use rayon::prelude::*;

#[cfg(not(test))]
const PARALLEL_INDEX_THRESHOLD: usize = 50_000;
#[cfg(test)]
const PARALLEL_INDEX_THRESHOLD: usize = 4;

/// Character span in the original candidate text that produced a generated key part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    /// Inclusive character offset where the source span starts.
    pub start_char: usize,
    /// Exclusive character offset where the source span ends.
    pub end_char: usize,
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

/// Text plus a per-character source map.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MappedText {
    /// Generated text.
    pub text: String,
    /// Map from generated text character positions back to source spans.
    pub source_map: Vec<Option<SourceSpan>>,
}

/// Helper for constructing mapped generated text.
#[derive(Clone, Debug, Default)]
pub struct MappedTextBuilder {
    mapped: MappedText,
}

impl MappedTextBuilder {
    /// Creates an empty mapped text builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends text and maps every appended character to the same source span.
    pub fn push_str(&mut self, text: &str, source: Option<SourceSpan>) {
        self.mapped.text.push_str(text);
        self.mapped.source_map.extend(text.chars().map(|_| source));
    }

    /// Appends one mapped character.
    pub fn push_char(&mut self, ch: char, source: Option<SourceSpan>) {
        self.mapped.text.push(ch);
        self.mapped.source_map.push(source);
    }

    /// Appends one unmapped separator character.
    pub fn push_unmapped_char(&mut self, ch: char) {
        self.mapped.text.push(ch);
        self.mapped.source_map.push(None);
    }

    /// Finishes the builder and returns the mapped text.
    pub fn finish(self) -> MappedText {
        self.mapped
    }
}

impl SearchKey {
    /// Creates a search key using the default weight for its kind.
    pub fn new(kind: KeyKind, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind,
            weight: Self::default_weight(kind),
            source_map: None,
        }
    }

    /// Returns the default score adjustment for a key kind.
    pub fn default_weight(kind: KeyKind) -> i32 {
        match kind {
            KeyKind::Original => 3000,
            KeyKind::Normalized => 2800,
            KeyKind::KanaReading => 1700,
            KeyKind::RomajiReading => 1800,
            KeyKind::PinyinFull => 1750,
            KeyKind::PinyinJoined => 1800,
            KeyKind::PinyinInitials => 1850,
            KeyKind::KoreanRomanized => 1800,
            KeyKind::KoreanInitials => 1850,
            KeyKind::KoreanKeyboard => 1750,
            KeyKind::LearnedAlias => 2500,
        }
    }

    /// Attaches a source map to this key.
    pub fn with_source_map(mut self, source_map: Vec<Option<SourceSpan>>) -> Self {
        self.source_map = Some(source_map.into_boxed_slice());
        self
    }

    /// Creates an original-display search key.
    pub fn original(text: impl Into<String>) -> Self {
        Self::new(KeyKind::Original, text)
    }

    /// Creates a normalized-display search key.
    pub fn normalized(text: impl Into<String>) -> Self {
        Self::new(KeyKind::Normalized, text)
    }

    /// Creates a Japanese kana-reading search key.
    pub fn kana_reading(text: impl Into<String>) -> Self {
        Self::new(KeyKind::KanaReading, text)
    }

    /// Creates a Japanese romaji-reading search key.
    pub fn romaji_reading(text: impl Into<String>) -> Self {
        Self::new(KeyKind::RomajiReading, text)
    }

    /// Creates a Chinese pinyin search key with separated syllables.
    pub fn pinyin_full(text: impl Into<String>) -> Self {
        Self::new(KeyKind::PinyinFull, text)
    }

    /// Creates a Chinese pinyin search key with syllables joined.
    pub fn pinyin_joined(text: impl Into<String>) -> Self {
        Self::new(KeyKind::PinyinJoined, text)
    }

    /// Creates a Chinese pinyin initials search key.
    pub fn pinyin_initials(text: impl Into<String>) -> Self {
        Self::new(KeyKind::PinyinInitials, text)
    }

    /// Creates a Korean romanized Hangul search key.
    pub fn korean_romanized(text: impl Into<String>) -> Self {
        Self::new(KeyKind::KoreanRomanized, text)
    }

    /// Creates a Korean initial-consonant search key.
    pub fn korean_initials(text: impl Into<String>) -> Self {
        Self::new(KeyKind::KoreanInitials, text)
    }

    /// Creates a Korean keyboard-layout search key.
    pub fn korean_keyboard(text: impl Into<String>) -> Self {
        Self::new(KeyKind::KoreanKeyboard, text)
    }

    /// Creates a user-learned alias search key.
    pub fn learned_alias(text: impl Into<String>) -> Self {
        Self::new(KeyKind::LearnedAlias, text)
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
    keys.extend(backend.build_candidate_keys(&display, config.key_budget()));
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
mod tests;
