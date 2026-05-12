use std::sync::Arc;

use yuru_core::{
    base_query_variants, KeyBudget, KeyKind, LangMode, LanguageBackend, PlainBackend, QueryBudget,
    QueryVariant, QueryVariantKind, SearchKey,
};
use yuru_ja::{JapaneseBackend, JapaneseReadingMode};
use yuru_ko::KoreanBackend;
use yuru_zh::{ChineseBackend, ChinesePolyphoneMode, ChineseScriptMode};

use crate::{
    cli::{Args, JaReadingArg, LangArg, ZhPolyphoneArg, ZhScriptArg},
    fields::InputItem,
};

pub(crate) fn create_backend(
    args: &Args,
    query: &str,
    items: &[InputItem],
) -> Arc<dyn LanguageBackend> {
    let lang = match args.lang {
        LangArg::Auto => detect_auto_lang(query, items),
        lang => lang,
    };

    match lang {
        LangArg::Plain => Arc::new(PlainBackend),
        LangArg::Ja => Arc::new(japanese_backend(args)),
        LangArg::Ko => Arc::new(korean_backend(args)),
        LangArg::Zh => Arc::new(chinese_backend(args)),
        LangArg::All => Arc::new(AllBackend::new(
            japanese_backend(args),
            korean_backend(args),
            chinese_backend(args),
        )),
        LangArg::Auto => unreachable!("auto language mode is resolved before backend creation"),
    }
}

fn japanese_backend(args: &Args) -> JapaneseBackend {
    JapaneseBackend::new(japanese_reading_mode(args.ja_reading))
}

fn korean_backend(args: &Args) -> KoreanBackend {
    KoreanBackend::new(
        args.ko_romanization && !args.no_ko_romanization,
        args.ko_initials && !args.no_ko_initials,
        args.ko_keyboard && !args.no_ko_keyboard,
    )
}

fn chinese_backend(args: &Args) -> ChineseBackend {
    ChineseBackend::new(
        args.zh_pinyin && !args.no_zh_pinyin,
        args.zh_initials && !args.no_zh_initials,
        chinese_polyphone_mode(args.zh_polyphone),
        chinese_script_mode(args.zh_script),
    )
}

#[derive(Clone, Debug)]
struct AllBackend {
    japanese: JapaneseBackend,
    korean: KoreanBackend,
    chinese: ChineseBackend,
}

impl AllBackend {
    fn new(japanese: JapaneseBackend, korean: KoreanBackend, chinese: ChineseBackend) -> Self {
        Self {
            japanese,
            korean,
            chinese,
        }
    }
}

impl LanguageBackend for AllBackend {
    fn mode(&self) -> LangMode {
        LangMode::All
    }

    fn build_candidate_keys(&self, text: &str, budget: KeyBudget) -> Vec<SearchKey> {
        let mut chinese_keys = self.chinese.build_candidate_keys(text, budget);
        prioritize_all_mode_keys(&mut chinese_keys);

        interleave_key_groups(
            [
                self.japanese.build_candidate_keys(text, budget),
                self.korean.build_candidate_keys(text, budget),
                chinese_keys,
            ],
            budget.max_keys,
        )
    }

    fn expand_query(&self, query: &str, budget: QueryBudget) -> Vec<QueryVariant> {
        let mut variants = base_query_variants(query);
        variants.extend(language_only_query_variants(
            self.chinese.expand_query(query, budget),
        ));
        variants.extend(language_only_query_variants(
            self.japanese.expand_query(query, budget),
        ));
        variants
    }
}

fn prioritize_all_mode_keys(keys: &mut [SearchKey]) {
    keys.sort_by_key(|key| match key.kind {
        KeyKind::PinyinInitials => 0,
        _ => 1,
    });
}

fn interleave_key_groups<const N: usize>(
    groups: [Vec<SearchKey>; N],
    max_keys: usize,
) -> Vec<SearchKey> {
    let mut indexes = [0usize; N];
    let mut out = Vec::new();

    loop {
        let mut progressed = false;
        for (group_index, group) in groups.iter().enumerate() {
            if out.len() >= max_keys {
                return out;
            }
            if let Some(key) = group.get(indexes[group_index]) {
                out.push(key.clone());
                indexes[group_index] += 1;
                progressed = true;
            }
        }
        if !progressed {
            return out;
        }
    }
}

fn language_only_query_variants(variants: Vec<QueryVariant>) -> impl Iterator<Item = QueryVariant> {
    variants.into_iter().filter(|variant| {
        !matches!(
            variant.kind,
            QueryVariantKind::Original | QueryVariantKind::Normalized
        )
    })
}

fn japanese_reading_mode(value: JaReadingArg) -> JapaneseReadingMode {
    match value {
        JaReadingArg::None => JapaneseReadingMode::None,
        JaReadingArg::Lindera => JapaneseReadingMode::Lindera,
    }
}

fn chinese_polyphone_mode(value: ZhPolyphoneArg) -> ChinesePolyphoneMode {
    match value {
        ZhPolyphoneArg::None => ChinesePolyphoneMode::None,
        ZhPolyphoneArg::Common => ChinesePolyphoneMode::Common,
        ZhPolyphoneArg::Phrase => ChinesePolyphoneMode::Phrase,
    }
}

fn chinese_script_mode(value: ZhScriptArg) -> ChineseScriptMode {
    match value {
        ZhScriptArg::Auto => ChineseScriptMode::Auto,
        ZhScriptArg::Hans => ChineseScriptMode::Hans,
        ZhScriptArg::Hant => ChineseScriptMode::Hant,
    }
}

fn detect_auto_lang(query: &str, items: &[InputItem]) -> LangArg {
    if contains_hangul(query) {
        return LangArg::Ko;
    }

    if yuru_core::normalize::contains_kana(query) {
        return LangArg::Ja;
    }

    let ascii_query = query.chars().any(|ch| ch.is_ascii_alphabetic())
        && query.chars().all(|ch| ch.is_ascii() || ch.is_whitespace());
    if !ascii_query {
        return LangArg::Plain;
    }

    let locale = locale_hint();
    let sample = items.iter().take(256);
    let mut sample_has_kana = false;
    let mut sample_has_han = false;
    let mut sample_has_hangul = false;
    for item in sample {
        sample_has_kana |= yuru_core::normalize::contains_kana(&item.search_text);
        sample_has_han |= contains_han(&item.search_text);
        sample_has_hangul |= contains_hangul(&item.search_text);
        if sample_has_kana && sample_has_han && sample_has_hangul {
            break;
        }
    }

    if locale.starts_with("ko") && sample_has_hangul {
        LangArg::Ko
    } else if sample_has_kana || locale.starts_with("ja") && sample_has_han {
        LangArg::Ja
    } else if locale.starts_with("zh") && sample_has_han {
        LangArg::Zh
    } else {
        LangArg::Plain
    }
}

pub(crate) fn locale_hint() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn contains_han(text: &str) -> bool {
    text.chars().any(|ch| {
        ('\u{3400}'..='\u{4dbf}').contains(&ch) || ('\u{4e00}'..='\u{9fff}').contains(&ch)
    })
}

fn contains_hangul(text: &str) -> bool {
    text.chars().any(|ch| {
        ('\u{1100}'..='\u{11ff}').contains(&ch)
            || ('\u{3130}'..='\u{318f}').contains(&ch)
            || ('\u{a960}'..='\u{a97f}').contains(&ch)
            || ('\u{ac00}'..='\u{d7a3}').contains(&ch)
            || ('\u{d7b0}'..='\u{d7ff}').contains(&ch)
    })
}

#[cfg(test)]
mod tests {
    use yuru_core::{build_candidate, KeyKind, SearchConfig};

    use super::*;

    #[test]
    fn all_backend_builds_japanese_korean_and_chinese_keys() {
        let backend = AllBackend::new(
            JapaneseBackend::default(),
            KoreanBackend::default(),
            ChineseBackend::default(),
        );
        let candidate = build_candidate(
            0,
            "カメラ 北京大学 한글",
            &backend,
            &SearchConfig::default(),
        );

        assert!(candidate
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::RomajiReading));
        assert!(candidate
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::KoreanRomanized));
        assert!(candidate
            .keys
            .iter()
            .any(|key| key.kind == KeyKind::PinyinInitials));
    }
}
