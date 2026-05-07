use std::sync::Arc;

use yuru_core::{LanguageBackend, PlainBackend};
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
        LangArg::Ja => Arc::new(JapaneseBackend::new(japanese_reading_mode(args.ja_reading))),
        LangArg::Ko => Arc::new(KoreanBackend::new(
            args.ko_romanization && !args.no_ko_romanization,
            args.ko_initials && !args.no_ko_initials,
            args.ko_keyboard && !args.no_ko_keyboard,
        )),
        LangArg::Zh => Arc::new(ChineseBackend::new(
            args.zh_pinyin && !args.no_zh_pinyin,
            args.zh_initials && !args.no_zh_initials,
            chinese_polyphone_mode(args.zh_polyphone),
            chinese_script_mode(args.zh_script),
        )),
        LangArg::Auto => unreachable!("auto language mode is resolved before backend creation"),
    }
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
