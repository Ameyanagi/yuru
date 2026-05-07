use crate::{query::key_kind_allowed, QueryVariant, SearchKey};
use nucleo_matcher::{Matcher, Utf32Str};
use unicode_normalization::UnicodeNormalization;

const SCORE_MATCH: i64 = 160;
const SCORE_GAP_START: i64 = -30;
const SCORE_GAP_EXTENSION: i64 = -10;
const BONUS_BOUNDARY: i64 = 80;
const BONUS_BOUNDARY_WHITE: i64 = 100;
const BONUS_BOUNDARY_DELIMITER: i64 = 90;
const BONUS_CAMEL_OR_NUMBER: i64 = 70;
const BONUS_CONSECUTIVE: i64 = 40;
const BONUS_FIRST_CHAR_MULTIPLIER: i64 = 2;
const START_POSITION_PENALTY: i64 = 2;
const TEXT_LENGTH_PENALTY_DIVISOR: i64 = 8;

/// Pluggable matcher that scores a pattern against one searchable text.
pub trait MatcherBackend {
    /// Returns a score when `pattern` matches `text`.
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64>;
}

/// Greedy subsequence matcher used by the default search path.
#[derive(Clone, Debug, Default)]
pub struct GreedyMatcher;

/// Exact substring matcher used by exact mode.
#[derive(Clone, Debug, Default)]
pub struct ExactMatcher;

/// Wrapper around `nucleo-matcher` with reusable UTF-32 buffers.
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

/// Character positions selected for highlighting a match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchPositions {
    /// Zero-based character indices in the original display text.
    pub char_indices: Vec<usize>,
}

impl MatchPositions {
    /// Returns true when no positions were selected.
    pub fn is_empty(&self) -> bool {
        self.char_indices.is_empty()
    }
}

/// Scores one query variant against one search key after compatibility checks.
pub fn score_key(variant: &QueryVariant, key: &SearchKey) -> Option<i64> {
    if !key_kind_allowed(variant, key.kind) {
        return None;
    }

    score_text(&variant.text, &key.text).map(|score| score + i64::from(key.weight + variant.weight))
}

/// Scores a fuzzy subsequence match between `pattern` and `text`.
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
    let text_chars: Vec<char> = text.chars().collect();
    let compact_score = compact_char_match_score(&pattern_chars, &text_chars)?;

    let exact_bonus = if pattern == text {
        10_000
    } else if text.starts_with(pattern) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    Some(exact_bonus + compact_score)
}

fn compact_char_match_score(pattern: &[char], text: &[char]) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }

    let mut pattern_index = 0usize;
    let mut end = None;
    for (text_index, text_ch) in text.iter().enumerate() {
        if pattern.get(pattern_index) == Some(text_ch) {
            pattern_index += 1;
            if pattern_index == pattern.len() {
                end = Some(text_index);
                break;
            }
        }
    }

    let mut text_index = end?;
    let mut score = 1000;
    let mut right_match: Option<usize> = None;
    let mut first = 0usize;
    for pattern_index in (0..pattern.len()).rev() {
        while text.get(text_index) != pattern.get(pattern_index) {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
        let position = text_index;
        first = position;

        score += SCORE_MATCH;
        let bonus = char_bonus_at(text, position);
        if pattern_index == 0 {
            score += bonus * BONUS_FIRST_CHAR_MULTIPLIER;
        } else {
            score += bonus;
        }

        if let Some(right_match) = right_match {
            if right_match == position + 1 {
                score += BONUS_CONSECUTIVE;
            } else {
                let gap = right_match.saturating_sub(position + 1) as i64;
                score += SCORE_GAP_START + SCORE_GAP_EXTENSION * gap.saturating_sub(1);
            }
        }
        right_match = Some(position);

        if pattern_index > 0 {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
    }

    Some(
        score
            - first as i64 * START_POSITION_PENALTY
            - text.len() as i64 / TEXT_LENGTH_PENALTY_DIVISOR,
    )
}

fn char_bonus_at(text: &[char], position: usize) -> i64 {
    if position == 0 {
        return BONUS_BOUNDARY_WHITE;
    }

    let previous = text[position - 1];
    let current = text[position];
    if previous.is_whitespace() {
        BONUS_BOUNDARY_WHITE
    } else if is_path_or_field_delimiter(previous) {
        BONUS_BOUNDARY_DELIMITER
    } else if !previous.is_alphanumeric() {
        BONUS_BOUNDARY
    } else if previous.is_lowercase() && current.is_uppercase()
        || !previous.is_numeric() && current.is_numeric()
    {
        BONUS_CAMEL_OR_NUMBER
    } else {
        0
    }
}

/// Finds character positions suitable for highlighting a matched pattern.
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
        for normalized in std::iter::once(ch).nfkc() {
            if case_sensitive {
                out.push((char_index, comparable_char(normalized)));
            } else {
                out.extend(
                    normalized
                        .to_lowercase()
                        .map(|lower| (char_index, comparable_char(lower))),
                );
            }
        }
    }
    out
}

fn comparable_char(ch: char) -> char {
    let folded = crate::normalize::fold_width_compatible_char(ch);
    if folded != ch {
        folded
    } else if ('ァ'..='ヶ').contains(&ch) {
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

    let compact_score = compact_ascii_match_score(pattern_bytes, text_bytes)?;

    let exact_bonus = if pattern_bytes == text_bytes {
        10_000
    } else if text_bytes.starts_with(pattern_bytes) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    Some(exact_bonus + compact_score)
}

fn compact_ascii_match_score(pattern: &[u8], text: &[u8]) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }

    let mut pattern_index = 0usize;
    let mut end = None;
    for (text_index, &text_byte) in text.iter().enumerate() {
        if pattern.get(pattern_index) == Some(&text_byte) {
            pattern_index += 1;
            if pattern_index == pattern.len() {
                end = Some(text_index);
                break;
            }
        }
    }

    let mut text_index = end?;
    let mut score = 1000;
    let mut right_match: Option<usize> = None;
    let mut first = 0usize;
    for pattern_index in (0..pattern.len()).rev() {
        while text.get(text_index) != pattern.get(pattern_index) {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
        let position = text_index;
        first = position;

        score += SCORE_MATCH;
        let bonus = ascii_bonus_at(text, position);
        if pattern_index == 0 {
            score += bonus * BONUS_FIRST_CHAR_MULTIPLIER;
        } else {
            score += bonus;
        }

        if let Some(right_match) = right_match {
            if right_match == position + 1 {
                score += BONUS_CONSECUTIVE;
            } else {
                let gap = right_match.saturating_sub(position + 1) as i64;
                score += SCORE_GAP_START + SCORE_GAP_EXTENSION * gap.saturating_sub(1);
            }
        }
        right_match = Some(position);

        if pattern_index > 0 {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
    }

    Some(
        score
            - first as i64 * START_POSITION_PENALTY
            - text.len() as i64 / TEXT_LENGTH_PENALTY_DIVISOR,
    )
}

fn ascii_bonus_at(text: &[u8], position: usize) -> i64 {
    if position == 0 {
        return BONUS_BOUNDARY_WHITE;
    }

    let previous = text[position - 1];
    let current = text[position];
    if previous.is_ascii_whitespace() {
        BONUS_BOUNDARY_WHITE
    } else if matches!(previous, b'/' | b'\\' | b',' | b':' | b';' | b'|') {
        BONUS_BOUNDARY_DELIMITER
    } else if !previous.is_ascii_alphanumeric() {
        BONUS_BOUNDARY
    } else if previous.is_ascii_lowercase() && current.is_ascii_uppercase()
        || !previous.is_ascii_digit() && current.is_ascii_digit()
    {
        BONUS_CAMEL_OR_NUMBER
    } else {
        0
    }
}

/// Scores an exact substring match between `pattern` and `text`.
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

fn is_path_or_field_delimiter(ch: char) -> bool {
    matches!(ch, '/' | '\\' | ',' | ':' | ';' | '|')
}

#[cfg(test)]
mod tests;
