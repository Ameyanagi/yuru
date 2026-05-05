//! Chinese pinyin matching backend for Yuru.
//!
//! The backend adds full pinyin, joined pinyin, and initials keys for Han text
//! and preserves source spans for CJK-aware highlighting.

pub mod pinyin;

use yuru_core::{base_query_variants, LangMode, LanguageBackend, QueryVariant, SearchKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChinesePolyphoneMode {
    None,
    Common,
    Phrase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChineseScriptMode {
    Auto,
    Hans,
    Hant,
}

#[derive(Clone, Debug)]
pub struct ChineseBackend {
    pinyin: bool,
    initials: bool,
    polyphone: ChinesePolyphoneMode,
    script: ChineseScriptMode,
}

impl ChineseBackend {
    pub fn new(
        pinyin: bool,
        initials: bool,
        polyphone: ChinesePolyphoneMode,
        script: ChineseScriptMode,
    ) -> Self {
        Self {
            pinyin,
            initials,
            polyphone,
            script,
        }
    }
}

impl Default for ChineseBackend {
    fn default() -> Self {
        Self {
            pinyin: true,
            initials: true,
            polyphone: ChinesePolyphoneMode::Common,
            script: ChineseScriptMode::Auto,
        }
    }
}

impl LanguageBackend for ChineseBackend {
    fn mode(&self) -> LangMode {
        LangMode::Chinese
    }

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey> {
        if !self.pinyin {
            return Vec::new();
        }

        let _polyphone = self.polyphone;
        let _script = self.script;

        pinyin::build_pinyin_keys_with_sources(text, 8)
            .into_iter()
            .filter_map(|key| {
                let search_key = if key.text.contains(' ') {
                    SearchKey::pinyin_full(key.text)
                } else if key.text.chars().count() <= text.chars().count() {
                    if !self.initials {
                        return None;
                    }
                    SearchKey::pinyin_initials(key.text)
                } else {
                    SearchKey::pinyin_joined(key.text)
                };
                Some(search_key.with_source_map(key.source_map))
            })
            .collect()
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
        let mut variants = base_query_variants(query);
        if !self.pinyin {
            return variants;
        }
        let normalized = yuru_core::normalize::normalize(query);
        if normalized.chars().all(|ch| ch.is_ascii_alphabetic()) && normalized.len() > 1 {
            if self.initials {
                variants.push(QueryVariant::initials(normalized.clone()));
            }
            variants.push(QueryVariant::pinyin(normalized));
        }
        variants
    }
}

#[cfg(test)]
mod tests {
    use yuru_core::{build_candidate, KeyKind, SearchConfig};

    use super::*;

    #[test]
    fn chinese_mode_does_not_build_japanese_reading_keys() {
        let backend = ChineseBackend::default();
        let cand = build_candidate(0, "北京大学", &backend, &SearchConfig::default());
        assert!(!cand
            .keys
            .iter()
            .any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading)));
    }

    #[test]
    fn chinese_mode_maps_initials_to_source_spans() {
        let backend = ChineseBackend::default();
        let cand = build_candidate(0, "北京大学", &backend, &SearchConfig::default());
        let key = cand
            .keys
            .iter()
            .find(|key| key.kind == KeyKind::PinyinInitials && key.text == "bjdx")
            .unwrap();

        assert_eq!(key.source_map.as_ref().unwrap()[0].unwrap().start, 0);
        assert_eq!(key.source_map.as_ref().unwrap()[1].unwrap().start, 1);
    }

    #[test]
    fn chinese_mode_can_disable_initials() {
        let backend = ChineseBackend::new(
            true,
            false,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Auto,
        );
        let cand = build_candidate(0, "北京大学", &backend, &SearchConfig::default());

        assert!(!cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::PinyinInitials));
        assert!(cand
            .keys
            .iter()
            .any(|key| matches!(key.kind, KeyKind::PinyinFull | KeyKind::PinyinJoined)));
    }
}
