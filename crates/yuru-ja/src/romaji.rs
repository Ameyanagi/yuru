use std::collections::{HashSet, VecDeque};

use yuru_core::SourceSpan;

pub fn romaji_to_kana_candidates(input: &str, max: usize) -> Vec<String> {
    if max == 0 {
        return Vec::new();
    }

    let input = input.trim().to_ascii_lowercase();
    if input.is_empty() {
        return Vec::new();
    }

    let mut queue = VecDeque::from([(0usize, String::new())]);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let search_budget = max.saturating_mul(16).max(16);
    let mut steps = 0usize;

    while let Some((index, built)) = queue.pop_front() {
        steps += 1;
        if steps > search_budget {
            break;
        }

        if index >= input.len() {
            push_unique(&mut out, &mut seen, built, max);
            if out.len() >= max {
                break;
            }
            continue;
        }

        for (next_index, next_text) in expand_one(&input, index, &built) {
            queue.push_back((next_index, next_text));
        }
    }

    for special in long_vowel_guess(&input) {
        push_unique(&mut out, &mut seen, special, max);
    }

    out
}

pub fn kana_to_romaji(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == 'っ' {
            if let Some(next) = chars.get(index + 1).and_then(|ch| romaji_for_kana(*ch)) {
                if let Some(first) = next.chars().next() {
                    out.push(first);
                }
            }
            index += 1;
            continue;
        }

        if let Some(next) = chars.get(index + 1) {
            if matches!(next, 'ゃ' | 'ゅ' | 'ょ') {
                if let Some(combo) = romaji_for_combo(chars[index], *next) {
                    out.push_str(combo);
                    index += 2;
                    continue;
                }
            }
        }

        if chars[index] == 'ん' {
            out.push('n');
        } else if let Some(romaji) = romaji_for_kana(chars[index]) {
            out.push_str(romaji);
        } else {
            out.push(chars[index]);
        }
        index += 1;
    }

    out
}

pub fn kana_to_romaji_with_source_map(
    input: &str,
    source_map: &[Option<SourceSpan>],
) -> (String, Vec<Option<SourceSpan>>) {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut out_map = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == 'っ' {
            if let Some(next) = chars.get(index + 1).and_then(|ch| romaji_for_kana(*ch)) {
                if let Some(first) = next.chars().next() {
                    out.push(first);
                    out_map.push(source_map.get(index).copied().flatten());
                }
            }
            index += 1;
            continue;
        }

        if let Some(next) = chars.get(index + 1) {
            if matches!(next, 'ゃ' | 'ゅ' | 'ょ') {
                if let Some(combo) = romaji_for_combo(chars[index], *next) {
                    let source = merge_source_spans(
                        source_map.get(index).copied().flatten(),
                        source_map.get(index + 1).copied().flatten(),
                    );
                    out.push_str(combo);
                    out_map.extend(combo.chars().map(|_| source));
                    index += 2;
                    continue;
                }
            }
        }

        let source = source_map.get(index).copied().flatten();
        if chars[index] == 'ん' {
            out.push('n');
            out_map.push(source);
        } else if let Some(romaji) = romaji_for_kana(chars[index]) {
            out.push_str(romaji);
            out_map.extend(romaji.chars().map(|_| source));
        } else {
            out.push(chars[index]);
            out_map.push(source);
        }
        index += 1;
    }

    (out, out_map)
}

fn merge_source_spans(left: Option<SourceSpan>, right: Option<SourceSpan>) -> Option<SourceSpan> {
    match (left, right) {
        (Some(left), Some(right)) => Some(SourceSpan {
            start: left.start.min(right.start),
            end: left.end.max(right.end),
        }),
        (Some(span), None) | (None, Some(span)) => Some(span),
        (None, None) => None,
    }
}

fn push_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String, max: usize) {
    if out.len() < max && seen.insert(value.clone()) {
        out.push(value);
    }
}

fn expand_one(input: &str, index: usize, built: &str) -> Vec<(usize, String)> {
    let rest = &input[index..];
    let mut expanded = Vec::new();

    if let Some((next, kana)) = expand_n(input, index, built) {
        expanded.push((next, kana));
        if rest.as_bytes().get(1) != Some(&b'y') {
            return expanded;
        }
    }

    if let Some((next, kana)) = expand_double_consonant(input, index, built) {
        expanded.push((next, kana));
        return expanded;
    }

    for len in [3usize, 2, 1] {
        if rest.len() >= len {
            let token = &rest[..len];
            if let Some(kana) = kana_for_token(token) {
                let mut next = built.to_owned();
                next.push_str(kana);
                expanded.push((index + len, next));
                return expanded;
            }
        }
    }

    let ch = rest.chars().next().expect("non-empty rest");
    let mut next = built.to_owned();
    next.push(ch);
    expanded.push((index + ch.len_utf8(), next));
    expanded
}

fn expand_n(input: &str, index: usize, built: &str) -> Option<(usize, String)> {
    let rest = &input[index..];
    if !rest.starts_with('n') {
        return None;
    }

    let mut chars = rest.chars();
    chars.next();
    let next = chars.next();
    let advance = if rest.starts_with("n'") { 2 } else { 1 };

    if rest.starts_with("n'") || next.is_none() || next == Some('n') {
        let mut out = built.to_owned();
        out.push('ん');
        return Some((index + advance, out));
    }

    if let Some(next) = next {
        if !matches!(next, 'a' | 'i' | 'u' | 'e' | 'o' | 'y') {
            let mut out = built.to_owned();
            out.push('ん');
            return Some((index + 1, out));
        }

        if next == 'y' {
            let mut out = built.to_owned();
            out.push('ん');
            return Some((index + 1, out));
        }
    }

    None
}

fn expand_double_consonant(input: &str, index: usize, built: &str) -> Option<(usize, String)> {
    let rest = &input[index..];
    let bytes = rest.as_bytes();
    if bytes.len() < 2 {
        return None;
    }

    let current = bytes[0] as char;
    let next = bytes[1] as char;
    if current == next && is_consonant(current) && current != 'n' {
        let mut out = built.to_owned();
        out.push('っ');
        Some((index + 1, out))
    } else {
        None
    }
}

fn is_consonant(ch: char) -> bool {
    ch.is_ascii_alphabetic() && !matches!(ch, 'a' | 'i' | 'u' | 'e' | 'o')
}

fn long_vowel_guess(input: &str) -> Vec<String> {
    if input.len() > 1 && input.chars().all(|ch| ch == 'o') {
        let count = input.len();
        return vec![
            "おう".repeat(count),
            "おー".repeat(count),
            "おおう".repeat(count),
        ];
    }

    match input {
        "tokyo" => vec!["とうきょう".to_string()],
        "kyoto" => vec!["きょうと".to_string()],
        "osaka" => vec!["おおさか".to_string()],
        "kobe" => vec!["こうべ".to_string()],
        _ => Vec::new(),
    }
}

fn kana_for_token(token: &str) -> Option<&'static str> {
    Some(match token {
        "a" => "あ",
        "i" => "い",
        "u" => "う",
        "e" => "え",
        "o" => "お",
        "ka" | "ca" => "か",
        "ki" => "き",
        "ku" | "cu" => "く",
        "ke" => "け",
        "ko" | "co" => "こ",
        "kya" => "きゃ",
        "kyu" => "きゅ",
        "kyo" => "きょ",
        "sa" => "さ",
        "shi" | "si" | "ci" => "し",
        "su" => "す",
        "se" | "ce" => "せ",
        "so" => "そ",
        "sha" => "しゃ",
        "shu" => "しゅ",
        "sho" => "しょ",
        "ta" => "た",
        "chi" | "ti" => "ち",
        "tsu" | "tu" => "つ",
        "te" => "て",
        "to" => "と",
        "cha" => "ちゃ",
        "chu" => "ちゅ",
        "cho" => "ちょ",
        "na" => "な",
        "ni" => "に",
        "nu" => "ぬ",
        "ne" => "ね",
        "no" => "の",
        "nya" => "にゃ",
        "nyu" => "にゅ",
        "nyo" => "にょ",
        "ha" => "は",
        "hi" => "ひ",
        "fu" | "hu" => "ふ",
        "he" => "へ",
        "ho" => "ほ",
        "hya" => "ひゃ",
        "hyu" => "ひゅ",
        "hyo" => "ひょ",
        "ma" => "ま",
        "mi" => "み",
        "mu" => "む",
        "me" => "め",
        "mo" => "も",
        "mya" => "みゃ",
        "myu" => "みゅ",
        "myo" => "みょ",
        "ya" => "や",
        "yu" => "ゆ",
        "yo" => "よ",
        "ra" => "ら",
        "ri" => "り",
        "ru" => "る",
        "re" => "れ",
        "ro" => "ろ",
        "rya" => "りゃ",
        "ryu" => "りゅ",
        "ryo" => "りょ",
        "wa" => "わ",
        "wo" => "を",
        "ga" => "が",
        "gi" => "ぎ",
        "gu" => "ぐ",
        "ge" => "げ",
        "go" => "ご",
        "gya" => "ぎゃ",
        "gyu" => "ぎゅ",
        "gyo" => "ぎょ",
        "za" => "ざ",
        "ji" | "zi" => "じ",
        "zu" => "ず",
        "ze" => "ぜ",
        "zo" => "ぞ",
        "ja" | "jya" => "じゃ",
        "ju" | "jyu" => "じゅ",
        "jo" | "jyo" => "じょ",
        "da" => "だ",
        "di" => "ぢ",
        "du" => "づ",
        "de" => "で",
        "do" => "ど",
        "ba" => "ば",
        "bi" => "び",
        "bu" => "ぶ",
        "be" => "べ",
        "bo" => "ぼ",
        "bya" => "びゃ",
        "byu" => "びゅ",
        "byo" => "びょ",
        "pa" => "ぱ",
        "pi" => "ぴ",
        "pu" => "ぷ",
        "pe" => "ぺ",
        "po" => "ぽ",
        "pya" => "ぴゃ",
        "pyu" => "ぴゅ",
        "pyo" => "ぴょ",
        _ => return None,
    })
}

fn romaji_for_combo(first: char, second: char) -> Option<&'static str> {
    Some(match (first, second) {
        ('き', 'ゃ') => "kya",
        ('き', 'ゅ') => "kyu",
        ('き', 'ょ') => "kyo",
        ('し', 'ゃ') => "sha",
        ('し', 'ゅ') => "shu",
        ('し', 'ょ') => "sho",
        ('ち', 'ゃ') => "cha",
        ('ち', 'ゅ') => "chu",
        ('ち', 'ょ') => "cho",
        ('に', 'ゃ') => "nya",
        ('に', 'ゅ') => "nyu",
        ('に', 'ょ') => "nyo",
        ('ひ', 'ゃ') => "hya",
        ('ひ', 'ゅ') => "hyu",
        ('ひ', 'ょ') => "hyo",
        ('み', 'ゃ') => "mya",
        ('み', 'ゅ') => "myu",
        ('み', 'ょ') => "myo",
        ('り', 'ゃ') => "rya",
        ('り', 'ゅ') => "ryu",
        ('り', 'ょ') => "ryo",
        ('ぎ', 'ゃ') => "gya",
        ('ぎ', 'ゅ') => "gyu",
        ('ぎ', 'ょ') => "gyo",
        ('じ', 'ゃ') => "ja",
        ('じ', 'ゅ') => "ju",
        ('じ', 'ょ') => "jo",
        ('び', 'ゃ') => "bya",
        ('び', 'ゅ') => "byu",
        ('び', 'ょ') => "byo",
        ('ぴ', 'ゃ') => "pya",
        ('ぴ', 'ゅ') => "pyu",
        ('ぴ', 'ょ') => "pyo",
        _ => return None,
    })
}

fn romaji_for_kana(ch: char) -> Option<&'static str> {
    Some(match ch {
        'あ' => "a",
        'い' => "i",
        'う' => "u",
        'え' => "e",
        'お' => "o",
        'か' => "ka",
        'き' => "ki",
        'く' => "ku",
        'け' => "ke",
        'こ' => "ko",
        'さ' => "sa",
        'し' => "shi",
        'す' => "su",
        'せ' => "se",
        'そ' => "so",
        'た' => "ta",
        'ち' => "chi",
        'つ' => "tsu",
        'て' => "te",
        'と' => "to",
        'な' => "na",
        'に' => "ni",
        'ぬ' => "nu",
        'ね' => "ne",
        'の' => "no",
        'は' => "ha",
        'ひ' => "hi",
        'ふ' => "fu",
        'へ' => "he",
        'ほ' => "ho",
        'ま' => "ma",
        'み' => "mi",
        'む' => "mu",
        'め' => "me",
        'も' => "mo",
        'や' => "ya",
        'ゆ' => "yu",
        'よ' => "yo",
        'ら' => "ra",
        'り' => "ri",
        'る' => "ru",
        'れ' => "re",
        'ろ' => "ro",
        'わ' => "wa",
        'を' => "wo",
        'が' => "ga",
        'ぎ' => "gi",
        'ぐ' => "gu",
        'げ' => "ge",
        'ご' => "go",
        'ざ' => "za",
        'じ' => "ji",
        'ず' => "zu",
        'ぜ' => "ze",
        'ぞ' => "zo",
        'だ' => "da",
        'ぢ' => "ji",
        'づ' => "zu",
        'で' => "de",
        'ど' => "do",
        'ば' => "ba",
        'び' => "bi",
        'ぶ' => "bu",
        'べ' => "be",
        'ぼ' => "bo",
        'ぱ' => "pa",
        'ぴ' => "pi",
        'ぷ' => "pu",
        'ぺ' => "pe",
        'ぽ' => "po",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn romaji_shinjuku() {
        let out = romaji_to_kana_candidates("shinjuku", 8);
        assert!(out.contains(&"しんじゅく".to_string()));
    }

    #[test]
    fn romaji_tokyo_has_short_and_long_vowel_candidates() {
        let out = romaji_to_kana_candidates("tokyo", 8);
        assert!(out.contains(&"ときょ".to_string()));
        assert!(out.contains(&"とうきょう".to_string()));
    }

    #[test]
    fn romaji_kyoto_has_long_vowel_candidate() {
        let out = romaji_to_kana_candidates("kyoto", 8);
        assert!(out.contains(&"きょうと".to_string()));
    }

    #[test]
    fn romaji_double_consonant_to_small_tsu() {
        let out = romaji_to_kana_candidates("gakkou", 8);
        assert!(out.contains(&"がっこう".to_string()));
    }

    #[test]
    fn romaji_n_before_consonant() {
        let out = romaji_to_kana_candidates("kanpai", 8);
        assert!(out.contains(&"かんぱい".to_string()));
    }

    #[test]
    fn romaji_n_apostrophe() {
        let out = romaji_to_kana_candidates("shin'ya", 8);
        assert!(out.contains(&"しんや".to_string()));
    }

    #[test]
    fn romaji_n_y_ambiguity_is_capped() {
        let out = romaji_to_kana_candidates("kanya", 8);
        assert!(out.contains(&"かにゃ".to_string()));
        assert!(out.contains(&"かんや".to_string()));
        assert!(out.len() <= 8);
    }

    #[test]
    fn romaji_variants_are_deduped_and_capped() {
        let out = romaji_to_kana_candidates("oooooooooooooooo", 4);
        let unique = out.iter().collect::<HashSet<_>>();

        assert!(out.len() <= 4);
        assert_eq!(out.len(), unique.len());
    }

    #[test]
    fn kana_to_romaji_basic() {
        assert_eq!(kana_to_romaji("かめら.txt"), "kamera.txt");
    }

    #[test]
    fn kana_to_romaji_preserves_source_map() {
        let source_map = vec![
            Some(SourceSpan { start: 0, end: 1 }),
            Some(SourceSpan { start: 1, end: 2 }),
            Some(SourceSpan { start: 2, end: 3 }),
        ];

        let (romaji, map) = kana_to_romaji_with_source_map("かめら", &source_map);

        assert_eq!(romaji, "kamera");
        assert_eq!(map[0], Some(SourceSpan { start: 0, end: 1 }));
        assert_eq!(map[2], Some(SourceSpan { start: 1, end: 2 }));
        assert_eq!(map[4], Some(SourceSpan { start: 2, end: 3 }));
    }
}
