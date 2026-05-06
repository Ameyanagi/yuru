//! Korean Hangul matching backend for Yuru.
//!
//! The backend adds deterministic Hangul romanization, choseong initials, and
//! Korean 2-set keyboard keys while preserving source spans for highlighting.

pub mod hangul;

use yuru_core::{base_query_variants, LangMode, LanguageBackend, QueryVariant, SearchKey};

#[derive(Clone, Debug)]
pub struct KoreanBackend {
    romanization: bool,
    initials: bool,
    keyboard: bool,
}

impl KoreanBackend {
    pub fn new(romanization: bool, initials: bool, keyboard: bool) -> Self {
        Self {
            romanization,
            initials,
            keyboard,
        }
    }
}

impl Default for KoreanBackend {
    fn default() -> Self {
        Self {
            romanization: true,
            initials: true,
            keyboard: true,
        }
    }
}

impl LanguageBackend for KoreanBackend {
    fn mode(&self) -> LangMode {
        LangMode::Korean
    }

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey> {
        hangul::build_korean_keys_with_sources(text, 8)
            .into_iter()
            .filter_map(|key| {
                let search_key = match key.kind {
                    hangul::KoreanKeyKind::Romanized => {
                        if !self.romanization {
                            return None;
                        }
                        SearchKey::korean_romanized(key.text)
                    }
                    hangul::KoreanKeyKind::Initials => {
                        if !self.initials {
                            return None;
                        }
                        SearchKey::korean_initials(key.text)
                    }
                    hangul::KoreanKeyKind::Keyboard => {
                        if !self.keyboard {
                            return None;
                        }
                        SearchKey::korean_keyboard(key.text)
                    }
                };
                Some(search_key.with_source_map(key.source_map))
            })
            .collect()
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
        base_query_variants(query)
    }
}

#[cfg(test)]
mod tests {
    use yuru_core::{build_candidate, KeyKind, SearchConfig};

    use super::*;

    #[test]
    fn korean_mode_builds_hangul_keys() {
        let backend = KoreanBackend::default();
        let cand = build_candidate(0, "한글.txt", &backend, &SearchConfig::default());

        assert!(cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanRomanized && key.text == "hangeul"));
        assert!(cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanInitials && key.text == "ㅎㄱ"));
        assert!(cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanKeyboard && key.text == "gksrmf"));
    }

    #[test]
    fn korean_mode_can_disable_each_generated_key_family() {
        let backend = KoreanBackend::new(false, true, false);
        let cand = build_candidate(0, "한글", &backend, &SearchConfig::default());

        assert!(cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanInitials));
        assert!(!cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanRomanized));
        assert!(!cand
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanKeyboard));
    }

    #[test]
    fn korean_mode_does_not_build_japanese_or_chinese_keys() {
        let backend = KoreanBackend::default();
        let cand = build_candidate(0, "한글", &backend, &SearchConfig::default());

        assert!(!cand.keys.iter().any(|key| matches!(
            key.kind,
            KeyKind::KanaReading
                | KeyKind::RomajiReading
                | KeyKind::PinyinFull
                | KeyKind::PinyinJoined
                | KeyKind::PinyinInitials
        )));
    }
}
