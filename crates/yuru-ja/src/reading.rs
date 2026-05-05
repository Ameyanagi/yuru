use std::collections::{HashSet, VecDeque};

use yuru_core::SourceSpan;

#[derive(Clone, Copy, Debug)]
struct ReadingEntry {
    surface: &'static str,
    readings: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingCandidate {
    pub text: String,
    pub source_map: Vec<Option<SourceSpan>>,
}

const READING_ENTRIES: &[ReadingEntry] = &[
    ReadingEntry {
        surface: "日本橋",
        readings: &["にほんばし", "にっぽんばし"],
    },
    ReadingEntry {
        surface: "日本語",
        readings: &["にほんご"],
    },
    ReadingEntry {
        surface: "日本人",
        readings: &["にほんじん", "にっぽんじん"],
    },
    ReadingEntry {
        surface: "東京駅",
        readings: &["とうきょうえき"],
    },
    ReadingEntry {
        surface: "東京",
        readings: &["とうきょう"],
    },
    ReadingEntry {
        surface: "新宿",
        readings: &["しんじゅく"],
    },
    ReadingEntry {
        surface: "京都",
        readings: &["きょうと"],
    },
    ReadingEntry {
        surface: "大阪",
        readings: &["おおさか"],
    },
    ReadingEntry {
        surface: "神戸",
        readings: &["こうべ"],
    },
    ReadingEntry {
        surface: "日本",
        readings: &["にほん", "にっぽん"],
    },
    ReadingEntry {
        surface: "語",
        readings: &["ご"],
    },
    ReadingEntry {
        surface: "人",
        readings: &["じん", "ひと"],
    },
    ReadingEntry {
        surface: "駅",
        readings: &["えき"],
    },
];

pub fn kanji_reading_candidates(input: &str, max: usize) -> Vec<String> {
    kanji_reading_candidates_with_sources(input, max)
        .into_iter()
        .map(|candidate| candidate.text)
        .collect()
}

pub fn kanji_reading_candidates_with_sources(input: &str, max: usize) -> Vec<ReadingCandidate> {
    if max == 0 || !contains_kanji(input) {
        return Vec::new();
    }

    let char_starts = char_start_byte_indices(input);
    let mut queue = VecDeque::from([(0usize, String::new(), Vec::new(), false)]);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let search_budget = max.saturating_mul(input.chars().count().max(1) * 4);
    let mut steps = 0usize;

    while let Some((byte_index, built, source_map, used_reading)) = queue.pop_front() {
        steps += 1;
        if steps > search_budget {
            break;
        }

        if byte_index >= input.len() {
            if used_reading && seen.insert(built.clone()) {
                out.push(ReadingCandidate {
                    text: built,
                    source_map,
                });
                if out.len() >= max {
                    break;
                }
            }
            continue;
        }

        let rest = &input[byte_index..];
        let char_index = byte_to_char_index(&char_starts, byte_index);
        let mut matched_entry = false;
        for entry in READING_ENTRIES {
            if !rest.starts_with(entry.surface) {
                continue;
            }

            matched_entry = true;
            let next_index = byte_index + entry.surface.len();
            let source_span = SourceSpan {
                start: char_index,
                end: char_index + entry.surface.chars().count(),
            };
            for reading in entry.readings {
                let mut next = built.clone();
                next.push_str(reading);
                let mut next_map = source_map.clone();
                next_map.extend(reading.chars().map(|_| Some(source_span)));
                queue.push_back((next_index, next, next_map, true));
            }
        }

        if !matched_entry {
            let ch = rest.chars().next().expect("non-empty rest");
            let mut next = built;
            next.push(ch);
            let mut next_map = source_map;
            next_map.push(Some(SourceSpan {
                start: char_index,
                end: char_index + 1,
            }));
            queue.push_back((byte_index + ch.len_utf8(), next, next_map, used_reading));
        }
    }

    out
}

fn char_start_byte_indices(input: &str) -> Vec<usize> {
    input
        .char_indices()
        .map(|(byte_index, _)| byte_index)
        .collect()
}

fn byte_to_char_index(char_starts: &[usize], byte_index: usize) -> usize {
    char_starts
        .binary_search(&byte_index)
        .unwrap_or_else(|index| index)
}

fn contains_kanji(input: &str) -> bool {
    input.chars().any(|ch| {
        ('\u{3400}'..='\u{4dbf}').contains(&ch) || ('\u{4e00}'..='\u{9fff}').contains(&ch)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_candidates_for_japanese_language_files() {
        let readings = kanji_reading_candidates("tests/日本語.txt", 8);
        assert!(readings.contains(&"tests/にほんご.txt".to_string()));

        let readings = kanji_reading_candidates("tests/日本人の.txt", 8);
        assert!(readings.contains(&"tests/にほんじんの.txt".to_string()));
    }

    #[test]
    fn reading_candidates_include_source_spans() {
        let readings = kanji_reading_candidates_with_sources("tests/日本人の.txt", 8);
        let reading = readings
            .iter()
            .find(|candidate| candidate.text == "tests/にほんじんの.txt")
            .unwrap();

        let chars: Vec<char> = reading.text.chars().collect();
        let ni_index = chars.iter().position(|&ch| ch == 'に').unwrap();
        let no_index = chars.iter().position(|&ch| ch == 'の').unwrap();

        assert_eq!(
            reading.source_map[ni_index],
            Some(SourceSpan { start: 6, end: 9 })
        );
        assert_eq!(
            reading.source_map[no_index],
            Some(SourceSpan { start: 9, end: 10 })
        );
    }

    #[test]
    fn reading_candidates_include_known_place_names() {
        let readings = kanji_reading_candidates("東京駅.txt", 8);
        assert!(readings.contains(&"とうきょうえき.txt".to_string()));

        let readings = kanji_reading_candidates("日本橋.txt", 8);
        assert!(readings.contains(&"にほんばし.txt".to_string()));
        assert!(readings.contains(&"にっぽんばし.txt".to_string()));
    }

    #[test]
    fn reading_candidates_are_capped_and_deduped() {
        let readings = kanji_reading_candidates("日本日本日本", 3);
        assert!(readings.len() <= 3);
        assert_eq!(
            readings.len(),
            readings.iter().collect::<HashSet<_>>().len()
        );
    }
}
