use pinyin::ToPinyin;
use yuru_core::SourceSpan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinyinKey {
    pub text: String,
    pub source_map: Vec<Option<SourceSpan>>,
}

pub fn build_pinyin_keys(text: &str, max: usize) -> Vec<String> {
    build_pinyin_keys_with_sources(text, max)
        .into_iter()
        .map(|key| key.text)
        .collect()
}

pub fn build_pinyin_keys_with_sources(text: &str, max: usize) -> Vec<PinyinKey> {
    if text.is_empty() || max == 0 {
        return Vec::new();
    }

    let syllables = extract_syllables(text);
    if syllables.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    push_unique(&mut out, full_pinyin_key(&syllables), max);
    push_unique(&mut out, joined_pinyin_key(&syllables), max);
    push_unique(&mut out, initials_pinyin_key(&syllables), max);
    out
}

fn extract_syllables(text: &str) -> Vec<(String, SourceSpan)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '重' && chars.get(index + 1) == Some(&'庆') {
            out.push((
                "chong".to_string(),
                SourceSpan {
                    start: index,
                    end: index + 1,
                },
            ));
            out.push((
                "qing".to_string(),
                SourceSpan {
                    start: index + 1,
                    end: index + 2,
                },
            ));
            index += 2;
            continue;
        }

        if let Some(pinyin) = chars[index].to_pinyin() {
            out.push((
                pinyin.plain().to_string(),
                SourceSpan {
                    start: index,
                    end: index + 1,
                },
            ));
        }
        index += 1;
    }

    out
}

fn full_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (index, (syllable, source)) in syllables.iter().enumerate() {
        if index > 0 {
            text.push(' ');
            source_map.push(None);
        }
        text.push_str(syllable);
        source_map.extend(syllable.chars().map(|_| Some(*source)));
    }

    PinyinKey { text, source_map }
}

fn joined_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (syllable, source) in syllables {
        text.push_str(syllable);
        source_map.extend(syllable.chars().map(|_| Some(*source)));
    }

    PinyinKey { text, source_map }
}

fn initials_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (syllable, source) in syllables {
        if let Some(initial) = syllable.chars().next() {
            text.push(initial);
            source_map.push(Some(*source));
        }
    }

    PinyinKey { text, source_map }
}

fn push_unique(out: &mut Vec<PinyinKey>, value: PinyinKey, max: usize) {
    if out.len() < max && !out.iter().any(|key| key.text == value.text) {
        out.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_beijing_university_keys() {
        let keys = build_pinyin_keys("北京大学", 8);
        assert!(keys.contains(&"bei jing da xue".to_string()));
        assert!(keys.contains(&"beijingdaxue".to_string()));
        assert!(keys.contains(&"bjdx".to_string()));
    }

    #[test]
    fn pinyin_chongqing_expected_common_reading() {
        let keys = build_pinyin_keys("重庆", 8);
        assert!(keys
            .iter()
            .any(|key| key.contains("chongqing") || key.contains("chong qing")));
    }

    #[test]
    fn pinyin_variants_are_capped() {
        let keys = build_pinyin_keys("重庆银行重庆分行", 4);
        assert!(keys.len() <= 4);
    }

    #[test]
    fn pinyin_empty_input_is_empty() {
        let keys = build_pinyin_keys("", 8);
        assert!(keys.is_empty());
    }

    #[test]
    fn pinyin_keys_include_source_maps() {
        let keys = build_pinyin_keys_with_sources("北京大学", 8);
        let initials = keys.iter().find(|key| key.text == "bjdx").unwrap();

        assert_eq!(initials.source_map.len(), 4);
        assert_eq!(
            initials.source_map[0],
            Some(SourceSpan { start: 0, end: 1 })
        );
        assert_eq!(
            initials.source_map[1],
            Some(SourceSpan { start: 1, end: 2 })
        );
        assert_eq!(
            initials.source_map[2],
            Some(SourceSpan { start: 2, end: 3 })
        );
        assert_eq!(
            initials.source_map[3],
            Some(SourceSpan { start: 3, end: 4 })
        );

        let full = keys
            .iter()
            .find(|key| key.text == "bei jing da xue")
            .unwrap();
        assert_eq!(full.source_map[3], None);
        assert_eq!(full.source_map[4], Some(SourceSpan { start: 1, end: 2 }));
    }
}
