use crate::{query::key_kind_allowed, QueryVariant, SearchKey};
use nucleo_matcher::{Matcher, Utf32Str};

pub trait MatcherBackend {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64>;
}

#[derive(Clone, Debug, Default)]
pub struct GreedyMatcher;

#[derive(Clone, Debug, Default)]
pub struct ExactMatcher;

#[derive(Clone, Debug, Default)]
pub struct NucleoMatcher {
    matcher: Matcher,
    pattern_buf: Vec<char>,
    text_buf: Vec<char>,
}

impl MatcherBackend for GreedyMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        score_text(pattern, text)
    }
}

impl MatcherBackend for ExactMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        score_exact_text(pattern, text)
    }
}

impl MatcherBackend for NucleoMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        let pattern = Utf32Str::new(pattern, &mut self.pattern_buf);
        let text = Utf32Str::new(text, &mut self.text_buf);
        self.matcher.fuzzy_match(text, pattern).map(i64::from)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchPositions {
    pub char_indices: Vec<usize>,
}

impl MatchPositions {
    pub fn is_empty(&self) -> bool {
        self.char_indices.is_empty()
    }
}

pub fn score_key(variant: &QueryVariant, key: &SearchKey) -> Option<i64> {
    if !key_kind_allowed(variant, key.kind) {
        return None;
    }

    score_text(&variant.text, &key.text).map(|score| score + i64::from(key.weight + variant.weight))
}

pub fn score_text(pattern: &str, text: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    if pattern.is_ascii() && text.is_ascii() {
        return score_ascii_text(pattern, text);
    }

    score_unicode_text(pattern, text)
}

fn score_unicode_text(pattern: &str, text: &str) -> Option<i64> {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let pattern_len = pattern_chars.len();
    let mut pattern_index = 0usize;
    let mut first = None;
    let mut last = 0usize;
    let mut previous_match = None;
    let mut previous_char = None;
    let mut consecutive = 0i64;
    let mut boundaries = 0i64;
    let mut text_len = 0usize;
    let mut chars = text.chars().enumerate();

    for (text_index, text_ch) in chars.by_ref() {
        text_len = text_index + 1;
        if pattern_chars.get(pattern_index) == Some(&text_ch) {
            if first.is_none() {
                first = Some(text_index);
            }
            if previous_match.is_some_and(|previous| text_index == previous + 1) {
                consecutive += 1;
            }
            if is_boundary_after(previous_char) {
                boundaries += 1;
            }

            previous_match = Some(text_index);
            last = text_index;
            pattern_index += 1;
            if pattern_index == pattern_len {
                break;
            }
        }
        previous_char = Some(text_ch);
    }

    if pattern_index != pattern_len {
        return None;
    }
    text_len += chars.count();

    let exact_bonus = if pattern == text {
        10_000
    } else if text.starts_with(pattern) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    let first = first.unwrap_or(0);
    let span = (last - first + 1) as i64;
    let gaps = span - pattern_len as i64;
    let text_len = text_len as i64;

    Some(1000 + exact_bonus + consecutive * 75 + boundaries * 150 - gaps * 12 - span * 3 - text_len)
}

pub fn match_positions(pattern: &str, text: &str, case_sensitive: bool) -> Option<MatchPositions> {
    if pattern.is_empty() {
        return Some(MatchPositions {
            char_indices: Vec::new(),
        });
    }

    let pattern = comparable_chars(pattern, case_sensitive);
    let text_comparable = comparable_indexed_chars(text, case_sensitive);
    let text_chars: Vec<char> = text.chars().collect();
    contiguous_text_positions(&pattern, &text_comparable)
        .or_else(|| best_subsequence_positions(&pattern, &text_comparable, &text_chars))
        .map(|char_indices| MatchPositions { char_indices })
}

fn comparable_chars(text: &str, case_sensitive: bool) -> Vec<char> {
    comparable_indexed_chars(text, case_sensitive)
        .into_iter()
        .map(|(_, ch)| ch)
        .collect()
}

fn comparable_indexed_chars(text: &str, case_sensitive: bool) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    for (char_index, ch) in text.chars().enumerate() {
        if case_sensitive {
            out.push((char_index, comparable_char(ch)));
        } else {
            out.extend(
                ch.to_lowercase()
                    .map(|lower| (char_index, comparable_char(lower))),
            );
        }
    }
    out
}

fn comparable_char(ch: char) -> char {
    if ('ァ'..='ヶ').contains(&ch) {
        char::from_u32(ch as u32 - 0x60).unwrap_or(ch)
    } else {
        ch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PositionCandidate {
    score: i64,
    positions: Vec<usize>,
}

fn best_subsequence_positions(
    pattern: &[char],
    text_comparable: &[(usize, char)],
    text_chars: &[char],
) -> Option<Vec<usize>> {
    if pattern.len() > text_comparable.len() {
        return None;
    }

    let mut states = Vec::new();
    for &(text_index, text_ch) in text_comparable {
        if pattern.first() == Some(&text_ch) {
            states.push(Some(PositionCandidate {
                score: match_position_score(text_chars, text_index) - text_index as i64 * 2,
                positions: vec![text_index],
            }));
        } else {
            states.push(None);
        }
    }

    for &pattern_ch in &pattern[1..] {
        let mut next_states = vec![None; text_comparable.len()];
        for (text_offset, &(text_index, text_ch)) in text_comparable.iter().enumerate() {
            if text_ch != pattern_ch {
                continue;
            }

            let mut best = None;
            for previous in states[..text_offset].iter().flatten() {
                let Some(&previous_index) = previous.positions.last() else {
                    continue;
                };
                if previous_index >= text_index {
                    continue;
                }

                let mut positions = previous.positions.clone();
                positions.push(text_index);
                let gap = text_index.saturating_sub(previous_index + 1) as i64;
                let consecutive_bonus = if text_index == previous_index + 1 {
                    160
                } else {
                    0
                };
                let score = previous.score
                    + match_position_score(text_chars, text_index)
                    + consecutive_bonus
                    - gap * 4;
                let candidate = PositionCandidate { score, positions };
                if best
                    .as_ref()
                    .is_none_or(|current| better_position_candidate(&candidate, current))
                {
                    best = Some(candidate);
                }
            }

            next_states[text_offset] = best;
        }

        states = next_states;
    }

    states
        .into_iter()
        .flatten()
        .max_by(compare_position_candidate)
        .map(|candidate| candidate.positions)
}

fn match_position_score(text_chars: &[char], position: usize) -> i64 {
    let boundary_bonus = if is_boundary(text_chars, position) {
        90
    } else {
        0
    };
    100 + boundary_bonus
}

fn better_position_candidate(left: &PositionCandidate, right: &PositionCandidate) -> bool {
    compare_position_candidate(left, right).is_gt()
}

fn compare_position_candidate(
    left: &PositionCandidate,
    right: &PositionCandidate,
) -> std::cmp::Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| span_len(right).cmp(&span_len(left)))
        .then_with(|| right.positions.cmp(&left.positions))
}

fn span_len(candidate: &PositionCandidate) -> usize {
    match (candidate.positions.first(), candidate.positions.last()) {
        (Some(first), Some(last)) => last - first + 1,
        _ => 0,
    }
}

fn contiguous_text_positions(
    pattern: &[char],
    text_comparable: &[(usize, char)],
) -> Option<Vec<usize>> {
    if pattern.len() > text_comparable.len() {
        return None;
    }

    text_comparable
        .windows(pattern.len())
        .find(|window| window.iter().map(|(_, ch)| ch).eq(pattern.iter()))
        .map(|window| window.iter().map(|(index, _)| *index).collect())
}

fn score_ascii_text(pattern: &str, text: &str) -> Option<i64> {
    let pattern_bytes = pattern.as_bytes();
    let text_bytes = text.as_bytes();

    if pattern_bytes.len() > text_bytes.len() {
        return None;
    }

    let mut pattern_index = 0usize;
    let mut first = usize::MAX;
    let mut last = 0usize;
    let mut previous = None;
    let mut consecutive = 0i64;
    let mut boundaries = 0i64;

    for (text_index, &text_byte) in text_bytes.iter().enumerate() {
        if pattern_bytes.get(pattern_index) == Some(&text_byte) {
            if first == usize::MAX {
                first = text_index;
            }
            if previous.is_some_and(|previous| text_index == previous + 1) {
                consecutive += 1;
            }
            if is_ascii_boundary(text_bytes, text_index) {
                boundaries += 1;
            }

            previous = Some(text_index);
            last = text_index;
            pattern_index += 1;
            if pattern_index == pattern_bytes.len() {
                break;
            }
        }
    }

    if pattern_index != pattern_bytes.len() {
        return None;
    }

    let exact_bonus = if pattern_bytes == text_bytes {
        10_000
    } else if text_bytes.starts_with(pattern_bytes) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    let span = (last - first + 1) as i64;
    let gaps = span - pattern_bytes.len() as i64;
    let text_len = text_bytes.len() as i64;

    Some(1000 + exact_bonus + consecutive * 75 + boundaries * 150 - gaps * 12 - span * 3 - text_len)
}

fn is_ascii_boundary(text: &[u8], position: usize) -> bool {
    position == 0 || matches!(text[position - 1], b'/' | b'\\' | b'_' | b'-' | b' ' | b'.')
}

fn is_boundary_after(previous_char: Option<char>) -> bool {
    previous_char.is_none_or(|ch| matches!(ch, '/' | '\\' | '_' | '-' | ' ' | '.'))
}

#[cfg(test)]
fn score_unicode_text_for_test(pattern: &str, text: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    if pattern_chars.len() > text_chars.len() {
        return None;
    }

    let mut matched_positions = Vec::with_capacity(pattern_chars.len());
    let mut pattern_index = 0usize;

    for (text_index, text_ch) in text_chars.iter().enumerate() {
        if Some(text_ch) == pattern_chars.get(pattern_index) {
            matched_positions.push(text_index);
            pattern_index += 1;
            if pattern_index == pattern_chars.len() {
                break;
            }
        }
    }

    if pattern_index != pattern_chars.len() {
        return None;
    }

    let exact_bonus = if pattern == text {
        10_000
    } else if text.starts_with(pattern) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    let consecutive_bonus = matched_positions
        .windows(2)
        .filter(|window| window[1] == window[0] + 1)
        .count() as i64
        * 75;
    let boundary_bonus = matched_positions
        .iter()
        .filter(|&&position| is_boundary(&text_chars, position))
        .count() as i64
        * 150;
    let first = *matched_positions.first().unwrap_or(&0);
    let last = *matched_positions.last().unwrap_or(&first);
    let span = (last - first + 1) as i64;
    let gaps = span - pattern_chars.len() as i64;
    let text_len = text_chars.len() as i64;

    Some(1000 + exact_bonus + consecutive_bonus + boundary_bonus - gaps * 12 - span * 3 - text_len)
}

pub fn score_exact_text(pattern: &str, text: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    let start = text.find(pattern)?;
    let exact_bonus = if pattern == text {
        10_000
    } else if start == 0 {
        8_000
    } else {
        6_000
    };
    Some(1000 + exact_bonus - start as i64 * 5 - text.chars().count() as i64)
}

fn is_boundary(text: &[char], position: usize) -> bool {
    position == 0 || matches!(text[position - 1], '/' | '\\' | '_' | '-' | ' ' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SearchKey;
    use proptest::prelude::*;

    #[test]
    fn subsequence_match_basic() {
        assert!(score_text("abc", "a_b_c").is_some());
        assert!(score_text("abc", "acb").is_none());
    }

    #[test]
    fn exact_match_scores_above_prefix_and_fuzzy() {
        let exact = score_text("abc", "abc").unwrap();
        let prefix = score_text("abc", "abcdef").unwrap();
        let fuzzy = score_text("abc", "a_b_c").unwrap();

        assert!(exact > prefix);
        assert!(prefix > fuzzy);
    }

    #[test]
    fn match_positions_tracks_subsequence_char_indices() {
        let positions = match_positions("abc", "a_b_c", true).unwrap();
        assert_eq!(positions.char_indices, vec![0, 2, 4]);
    }

    #[test]
    fn match_positions_can_ignore_case() {
        let positions = match_positions("read", "README.md", false).unwrap();
        assert_eq!(positions.char_indices, vec![0, 1, 2, 3]);
        assert!(match_positions("read", "README.md", true).is_none());
    }

    #[test]
    fn match_positions_treats_hiragana_and_katakana_as_equivalent() {
        let positions = match_positions("かめら", "カメラ.txt", false).unwrap();
        assert_eq!(positions.char_indices, vec![0, 1, 2]);
    }

    #[test]
    fn match_positions_prefers_better_chunk_over_first_subsequence() {
        let positions = match_positions("bsea", "benches/search.rs", false).unwrap();
        assert_eq!(positions.char_indices, vec![0, 8, 9, 10]);
    }

    #[test]
    fn ascii_fast_path_matches_unicode_path_score() {
        for (pattern, text) in [
            ("abc", "abc"),
            ("abc", "abcdef"),
            ("abc", "a_b_c"),
            ("read", "src/module_42/README.md"),
        ] {
            assert_eq!(
                score_text(pattern, text),
                score_unicode_text_for_test(pattern, text)
            );
        }
    }

    #[test]
    fn exact_match_requires_contiguous_text() {
        assert!(score_exact_text("abc", "abc.txt").is_some());
        assert!(score_exact_text("abc", "a_b_c").is_none());
    }

    #[test]
    fn reading_match_scores_below_original_exact() {
        let original = score_key(
            &QueryVariant::original("tokyo"),
            &SearchKey::original("tokyo"),
        )
        .unwrap();
        let reading = score_key(
            &QueryVariant::original("tokyo"),
            &SearchKey::romaji_reading("tokyoeki"),
        )
        .unwrap();

        assert!(original > reading);
    }

    #[test]
    fn learned_alias_scores_high_enough() {
        let alias = score_key(
            &QueryVariant::original("nihonbashi"),
            &SearchKey::learned_alias("nihonbashi"),
        )
        .unwrap();
        let reading = score_key(
            &QueryVariant::original("nihonbashi"),
            &SearchKey::romaji_reading("nihonbashieki"),
        )
        .unwrap();

        assert!(alias >= reading);
    }

    #[test]
    fn nucleo_matcher_scores_subsequence() {
        let mut matcher = NucleoMatcher::default();

        assert!(matcher.score("rdme", "README.md").is_some());
        assert!(matcher.score("zz", "README.md").is_none());
    }

    proptest! {
        #[test]
        fn score_text_never_panics(pattern in "\\PC{0,24}", text in "\\PC{0,64}") {
            let _ = score_text(&pattern, &text);
            let _ = score_exact_text(&pattern, &text);
        }

        #[test]
        fn match_positions_are_ordered_and_in_bounds(
            pattern in "\\PC{0,24}",
            text in "\\PC{0,64}",
            case_sensitive in any::<bool>(),
        ) {
            if let Some(positions) = match_positions(&pattern, &text, case_sensitive) {
                let text_len = text.chars().count();
                prop_assert!(positions.char_indices.windows(2).all(|window| window[0] < window[1]));
                prop_assert!(positions.char_indices.iter().all(|index| *index < text_len));
            }
        }
    }
}
