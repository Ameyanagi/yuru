//! Chinese pinyin matching backend for Yuru.
//!
//! The backend adds full pinyin, joined pinyin, and initials keys for Han text
//! and preserves source spans for CJK-aware highlighting.

/// Pinyin key generation helpers.
pub mod pinyin;

use yuru_core::{base_query_variants, LangMode, LanguageBackend, QueryVariant, SearchKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Controls how alternate Chinese character readings are generated.
pub enum ChinesePolyphoneMode {
    /// Use only the primary reading for each character.
    None,
    /// Add common alternate readings with a small cap.
    Common,
    /// Reserved phrase mode; currently falls back to common alternate readings.
    Phrase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Reserved Chinese script handling mode.
pub enum ChineseScriptMode {
    /// Reserved auto script handling.
    Auto,
    /// Reserved simplified Chinese handling.
    Hans,
    /// Reserved traditional Chinese handling.
    Hant,
}

#[derive(Clone, Debug)]
/// Chinese language backend for pinyin and initials keys.
pub struct ChineseBackend {
    pinyin: bool,
    initials: bool,
    polyphone: ChinesePolyphoneMode,
    script: ChineseScriptMode,
}

impl ChineseBackend {
    /// Creates a Chinese backend with selected pinyin and script options.
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

        let _script = self.script;

        pinyin::build_pinyin_keys_with_sources_for_mode(text, 8, self.polyphone)
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

    #[test]
    fn chinese_polyphone_none_and_common_generate_different_keys() {
        let none = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::None,
            ChineseScriptMode::Auto,
        );
        let common = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Auto,
        );

        let cand_none = build_candidate(0, "还没", &none, &SearchConfig::default());
        let cand_common = build_candidate(0, "还没", &common, &SearchConfig::default());
        let none_texts: Vec<_> = cand_none.keys.iter().map(|key| key.text.as_str()).collect();
        let common_texts: Vec<_> = cand_common
            .keys
            .iter()
            .map(|key| key.text.as_str())
            .collect();

        assert!(none_texts.contains(&"haimei"));
        assert!(!none_texts.contains(&"huanmei"));
        assert!(common_texts.contains(&"haimei"));
        assert!(common_texts.contains(&"huanmei"));
    }

    #[test]
    fn chinese_polyphone_phrase_matches_common_backend_keys() {
        let common = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Auto,
        );
        let phrase = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Phrase,
            ChineseScriptMode::Auto,
        );

        let cand_common = build_candidate(0, "还没", &common, &SearchConfig::default());
        let cand_phrase = build_candidate(0, "还没", &phrase, &SearchConfig::default());
        let common_texts: Vec<_> = cand_common
            .keys
            .iter()
            .map(|key| key.text.as_str())
            .collect();
        let phrase_texts: Vec<_> = cand_phrase
            .keys
            .iter()
            .map(|key| key.text.as_str())
            .collect();

        assert_eq!(common_texts, phrase_texts);
    }

    #[test]
    fn chinese_script_modes_do_not_change_generated_keys() {
        let auto = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Auto,
        );
        let hans = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Hans,
        );
        let hant = ChineseBackend::new(
            true,
            true,
            ChinesePolyphoneMode::Common,
            ChineseScriptMode::Hant,
        );

        let auto_keys = auto.build_candidate_keys("臺灣");
        let hans_keys = hans.build_candidate_keys("臺灣");
        let hant_keys = hant.build_candidate_keys("臺灣");

        assert_eq!(auto_keys, hans_keys);
        assert_eq!(auto_keys, hant_keys);
    }
}
