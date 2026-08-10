use crate::{query::key_kind_allowed, QueryVariant, SearchKey};
use nucleo_matcher::{chars, Config as NucleoConfig, Matcher, Utf32Str};
use unicode_normalization::UnicodeNormalization;

const SCORE_MATCH: i64 = 160;
const SCORE_GAP_START: i64 = -30;
const SCORE_GAP_EXTENSION: i64 = -10;
const BONUS_BOUNDARY: i64 = 80;
const BONUS_BOUNDARY_WHITE: i64 = 100;
const BONUS_BOUNDARY_DELIMITER: i64 = 90;
const BONUS_CAMEL_OR_NUMBER: i64 = 70;
const BONUS_CONSECUTIVE: i64 = 40;
/// Bonus for a case-insensitive match whose characters are the ones the query was typed
/// with, so that a candidate spelled the way the user spelled it wins a tie.
///
/// Awarded once per match rather than per matched character: per character it would scale
/// with query length, and a long query would swamp every boundary bonus. Sized to sit
/// between [`BONUS_CAMEL_OR_NUMBER`] and [`BONUS_BOUNDARY`], so a literally spelled match
/// outranks a camelCase hump but never outranks a real word boundary. Inert when the search
/// is case-sensitive, where every match is exact by construction.
///
/// This states, and weakens, a preference v0.1.11 got by accident: case-insensitive matching
/// used to reach mixed-case text through the lower-weighted [`crate::KeyKind::Normalized`]
/// key (2800) while a literal match came from the [`crate::KeyKind::Original`] key (3000), so
/// the literal spelling won by 200 points. Folding case inside the matcher retired that
/// accident - together with the boundary bonuses the lowercased key was destroying - and left
/// nothing preferring the spelling the user typed. 75 points is what is left of the 200.
///
/// The extended-query path awards the same bonus on the same terms, from
/// `fzf_query::case_exact_bonus`, so `'foo` and `--exact foo` order case variants alike.
pub(crate) const BONUS_CASE_EXACT: i64 = 75;
const _: () = assert!(
    BONUS_CASE_EXACT > BONUS_CAMEL_OR_NUMBER && BONUS_CASE_EXACT < BONUS_BOUNDARY,
    "the exact-case bonus must break a camelCase tie without outranking a word boundary"
);
const BONUS_FIRST_CHAR_MULTIPLIER: i64 = 2;
const START_POSITION_PENALTY: i64 = 2;
const TEXT_LENGTH_PENALTY_DIVISOR: i64 = 8;

/// Pluggable matcher that scores a pattern against one searchable text.
///
/// Case handling belongs to the implementation: [`crate::search`] builds its matcher from
/// [`crate::SearchConfig::case_sensitive`], and a matcher passed to
/// [`crate::search_with_stats`] carries whatever case policy it was built with. Search only
/// assumes a case policy where the implementation states one through [`Self::folds_case`].
pub trait MatcherBackend {
    /// Returns a score when `pattern` matches `text`.
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64>;

    /// Returns whether [`Self::score`] folds case with the same mapping the index used,
    /// so that a [`crate::SearchKey`] which is only a case-folded copy of the display text
    /// cannot match where the display text does not.
    ///
    /// The mapping is `fold_case_char`: one character in, one character out, `char`'s
    /// simple lowercase mapping where that is a single character and the character as
    /// written otherwise. Returning `true` promises that
    /// `score(pattern, fold(text)).is_some()` implies `score(pattern, text).is_some()` for
    /// every pattern, which lets search skip keys flagged
    /// [`crate::SearchKey::case_fold_only`] and score only the higher-weighted original key.
    ///
    /// The default is `false`: a matcher that never says otherwise is offered every key,
    /// including the case-folded one, so a matcher that is case-sensitive by construction
    /// still finds case-insensitive matches through it. Return `!case_sensitive` only if the
    /// implementation really folds with that mapping - a matcher with its own folding table
    /// must keep the default, because a table that differs anywhere would silently drop the
    /// matches only the folded key can reach.
    fn folds_case(&self) -> bool {
        false
    }
}

/// Greedy subsequence matcher used by the default search path.
#[derive(Clone, Copy, Debug, Default)]
pub struct GreedyMatcher {
    /// Compares characters as written instead of case-folding them.
    pub case_sensitive: bool,
}

/// Exact substring matcher used by exact mode.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactMatcher {
    /// Compares characters as written instead of case-folding them.
    pub case_sensitive: bool,
}

impl GreedyMatcher {
    /// Creates a greedy matcher with the given case policy.
    pub fn new(case_sensitive: bool) -> Self {
        Self { case_sensitive }
    }
}

impl ExactMatcher {
    /// Creates an exact matcher with the given case policy.
    pub fn new(case_sensitive: bool) -> Self {
        Self { case_sensitive }
    }
}

/// Wrapper around `nucleo-matcher` with reusable UTF-32 buffers.
///
/// The case policy lives in the wrapped `nucleo_matcher::Config`, so the *haystack* side is
/// compared by nucleo itself rather than pre-folded here: folding the text before handing it
/// over would move the match positions and the boundary bonuses off the text as written.
/// The *needle* side is the caller's job - see [`MatcherBackend::score`] below.
/// [`Default`] is case-insensitive, matching `nucleo_matcher::Config::DEFAULT`.
#[derive(Clone, Debug)]
pub struct NucleoMatcher {
    matcher: Matcher,
    pattern_buf: Vec<char>,
    text_buf: Vec<char>,
    folded: FoldedPattern,
}

impl NucleoMatcher {
    /// Creates a nucleo matcher with the given case policy.
    pub fn new(case_sensitive: bool) -> Self {
        let mut config = NucleoConfig::DEFAULT;
        config.ignore_case = !case_sensitive;
        Self {
            matcher: Matcher::new(config),
            pattern_buf: Vec::new(),
            text_buf: Vec::new(),
            folded: FoldedPattern::default(),
        }
    }
}

/// One-entry memo of the case-folded form of the last pattern [`NucleoMatcher`] was asked for.
///
/// A search reuses one matcher across every candidate and scores only a handful of distinct
/// patterns (the query variants), so a single slot hits essentially always. `source` doubles
/// as the cache key and `text` holds the fold, empty when the pattern needed none, which is
/// the common case for a lowercase query.
#[derive(Clone, Debug, Default)]
struct FoldedPattern {
    source: String,
    text: String,
    needed: bool,
}

impl FoldedPattern {
    /// Returns `pattern` folded with nucleo's own case-folding table, or `pattern` itself when
    /// nucleo's table has nothing to fold in it.
    #[inline]
    fn pattern<'a>(&'a mut self, pattern: &'a str) -> &'a str {
        if self.source != pattern {
            self.source.clear();
            self.source.push_str(pattern);
            self.needed = pattern.chars().any(chars::is_upper_case);
            if self.needed {
                self.text.clear();
                self.text.extend(pattern.chars().map(chars::to_lower_case));
            }
        }

        if self.needed {
            &self.text
        } else {
            pattern
        }
    }
}

impl Default for NucleoMatcher {
    fn default() -> Self {
        Self::new(false)
    }
}

impl MatcherBackend for GreedyMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        score_text(pattern, text, self.case_sensitive)
    }

    /// [`score_text`] compares characters through `fold_case_char`, which is the mapping
    /// [`crate::SearchKey::case_fold_only`] is computed with. Its expanded retry keeps that
    /// promise - see [`retry_with_lowercase_expansion`].
    fn folds_case(&self) -> bool {
        !self.case_sensitive
    }
}

impl MatcherBackend for ExactMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        score_exact_text(pattern, text, self.case_sensitive)
    }

    /// [`score_exact_text`] compares characters through `fold_case_char`, which is the
    /// mapping [`crate::SearchKey::case_fold_only`] is computed with. Its expanded retry keeps
    /// that promise - see [`retry_with_lowercase_expansion`].
    fn folds_case(&self) -> bool {
        !self.case_sensitive
    }
}

impl MatcherBackend for NucleoMatcher {
    /// `nucleo_matcher::Matcher` documents that the needle "must always be normalized by the
    /// caller (unicode normalization and case folding)", so an `ignore_case` matcher has to be
    /// handed an already-folded pattern: its haystack comparison folds only the haystack and
    /// then expects the needle to already be lower case. An unfolded needle does not merely
    /// miss - with an all-ASCII needle and haystack, nucleo's prefilter and its match matrix
    /// disagree and `fuzzy_optimal.rs` panics with "should have been caught by prefilter".
    ///
    /// Folded with nucleo's own `to_lower_case` so the two sides use one table. This is the
    /// needle only; the haystack still reaches nucleo as written.
    ///
    /// A search calls this once per key per candidate with only a handful of distinct
    /// patterns, so the decision is memoized on the pattern rather than recomputed: deciding
    /// it costs a table binary search per character, which measured 1.24x on a 500k search
    /// when paid on every call.
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        let Self {
            matcher,
            pattern_buf,
            text_buf,
            folded,
        } = self;

        let pattern = if matcher.config.ignore_case {
            folded.pattern(pattern)
        } else {
            pattern
        };

        let pattern = Utf32Str::new(pattern, pattern_buf);
        let text = Utf32Str::new(text, text_buf);
        matcher.fuzzy_match(text, pattern).map(i64::from)
    }

    /// Keeps the conservative default even when this matcher is configured case-insensitive:
    /// `nucleo-matcher` folds with its own simple-case-folding table, which disagrees with
    /// `fold_case_char` for 55 characters (`Ɤ` U+A7CB, `Ᲊ` U+1C89, the Garay block, ...) that
    /// its table does not know. Claiming to fold case would drop the only key that reaches
    /// those characters - `yuru --algo v2 --filter ɤ` over `Ɤx` must still match.
    ///
    /// Deliberately not `!case_sensitive`: this is a claim about the folding *mapping*, not
    /// about whether folding happens at all, and the mapping is wrong either way. A
    /// case-sensitive nucleo matcher wants `false` too, since it folds nothing.
    fn folds_case(&self) -> bool {
        false
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
pub fn score_key(variant: &QueryVariant, key: &SearchKey, case_sensitive: bool) -> Option<i64> {
    if !key_kind_allowed(variant, key.kind) {
        return None;
    }

    score_text(&variant.text, &key.text, case_sensitive)
        .map(|score| score + i64::from(key.weight + variant.weight))
}

/// Scores a fuzzy subsequence match between `pattern` and `text`.
///
/// With `case_sensitive` false both sides are case-folded while comparing, while
/// boundary and camel-case bonuses still read the text as written, and a match that needed
/// no folding at all collects [`BONUS_CASE_EXACT`].
pub fn score_text(pattern: &str, text: &str, case_sensitive: bool) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    if pattern.is_ascii() && text.is_ascii() {
        // ASCII cannot hold `MULTI_CHAR_LOWERCASE`, so the retry below cannot apply and this
        // path stays exactly as cheap as it was.
        return if case_sensitive {
            score_ascii_text::<true>(pattern, text)
        } else {
            score_ascii_text::<false>(pattern, text)
        };
    }

    if case_sensitive {
        return score_unicode_text::<true>(pattern, text);
    }

    score_unicode_text::<false>(pattern, text)
        .or_else(|| retry_with_lowercase_expansion(pattern, text, score_unicode_text::<false>))
}

/// Folds one character for comparison, leaving it as written when case matters.
///
/// Folding is deliberately one character in, one character out: every caller compares
/// characters pairwise and reports positions in the unfolded text, so an expansion would
/// desynchronize both. A character whose full lowercase mapping is several characters
/// (`İ` lowercases to `i` followed by U+0307 COMBINING DOT ABOVE) therefore stays as
/// written rather than folding to its first character, which would make `İ` compare equal
/// to a bare `i` and match patterns the character does not contain.
///
/// Refusing to fold it here is not the same as not folding it: case-insensitive matching
/// reaches the full lowercase form through [`retry_with_lowercase_expansion`], which writes
/// the mapping out on copies of both sides before comparing them, and through the
/// [`crate::KeyKind::Normalized`] key, whose text already carries the expansion.
fn fold_char<const CASE_SENSITIVE: bool>(ch: char) -> char {
    if CASE_SENSITIVE {
        ch
    } else if ch.is_ascii() {
        ch.to_ascii_lowercase()
    } else {
        let mut lower = ch.to_lowercase();
        match (lower.len(), lower.next()) {
            (1, Some(lower)) => lower,
            _ => ch,
        }
    }
}

/// Folds one character the way case-insensitive matching compares it.
pub(crate) fn fold_case_char(ch: char) -> char {
    fold_char::<false>(ch)
}

/// The only character whose full lowercase mapping is longer than one character, and so the
/// only one [`fold_char`] refuses to fold.
///
/// `İ` U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE lowercases to `i` followed by U+0307
/// COMBINING DOT ABOVE. An exhaustive walk of `char::to_lowercase` over the whole scalar
/// range finds no second one; `only_one_character_has_a_multi_character_lowercase_mapping`
/// pins that so a future Unicode table update cannot quietly add one.
const MULTI_CHAR_LOWERCASE: char = 'İ';

/// [`MULTI_CHAR_LOWERCASE`]'s full lowercase mapping, written out.
const MULTI_CHAR_LOWERCASE_EXPANSION: &str = "i\u{307}";

/// First UTF-8 byte of [`MULTI_CHAR_LOWERCASE`], which is what
/// [`expand_multi_char_lowercase`] looks for.
///
/// A lead byte, never a continuation byte, so a `memchr` for it does not keep stopping on
/// unrelated multi-byte characters - the trailing `0xB0` is a perfectly ordinary
/// continuation byte and searching for that instead cost CJK searches 3-7%.
const MULTI_CHAR_LOWERCASE_LEAD_BYTE: u8 = 0xC4;
const _: () = assert!(
    (MULTI_CHAR_LOWERCASE as u32) >= 0x80
        && (MULTI_CHAR_LOWERCASE as u32) < 0x800
        && MULTI_CHAR_LOWERCASE_LEAD_BYTE == 0xC0 | ((MULTI_CHAR_LOWERCASE as u32) >> 6) as u8,
    "the lead byte must be the one a two-byte UTF-8 encoding of the character starts with"
);

/// Retries a case-insensitive comparison that failed, with [`MULTI_CHAR_LOWERCASE`] written
/// out on both sides, and returns `None` when there was nothing to write out.
///
/// This is how case-insensitive matching folds the one character [`fold_char`] cannot:
/// pairwise folding must stay one character in and one character out to keep every caller's
/// indices lined up with the text as written, so the expansion happens *before* comparing
/// instead, on copies, where both sides expand alike and every remaining fold is 1:1 again.
///
/// Deliberately a retry rather than a pre-pass. Every text that already matched keeps the
/// score it had, since the expanded comparison is only reached when the as-written one found
/// nothing at all, so this can only turn a false negative into a match. It also keeps the
/// cost off the hot path: nothing here runs until a comparison has already failed, and then
/// only two `memchr`s, which for the overwhelmingly common text without a `İ` in it is all
/// that happens.
///
/// The indices the expanded comparison computes internally are indices into the expanded
/// copies, which is why only score-only entry points may use it. [`score_text`] and
/// [`score_exact_text`] both return nothing but a score. [`match_positions`], which does
/// report indices into its argument, handles the expansion itself by carrying the unexpanded
/// character index alongside each expanded character.
///
/// [`MatcherBackend::folds_case`] stays true for the matchers that do this. Its promise is
/// that a hit against the 1:1-folded text implies a hit against the text as written, and
/// expansion preserves it: [`fold_char`] leaves [`MULTI_CHAR_LOWERCASE`] alone, so writing it
/// out and folding 1:1 commute, and the expanded copy of a folded text is the 1:1 fold of the
/// expanded text - the same comparison either way.
fn retry_with_lowercase_expansion(
    pattern: &str,
    text: &str,
    score: impl FnOnce(&str, &str) -> Option<i64>,
) -> Option<i64> {
    let expanded_pattern = expand_multi_char_lowercase(pattern);
    let expanded_text = expand_multi_char_lowercase(text);
    if expanded_pattern.is_none() && expanded_text.is_none() {
        return None;
    }

    score(
        expanded_pattern.as_deref().unwrap_or(pattern),
        expanded_text.as_deref().unwrap_or(text),
    )
}

/// Returns `text` with [`MULTI_CHAR_LOWERCASE`] written out, or `None` when it holds none.
///
/// The byte search is the gate every failed comparison pays; `slice::contains` over `u8` is
/// specialized to `memchr`. Only text that holds the lead byte at all - `İ` itself or one of
/// the 63 other Latin Extended-A characters that share it - pays for the character search
/// that confirms it.
fn expand_multi_char_lowercase(text: &str) -> Option<String> {
    (text.as_bytes().contains(&MULTI_CHAR_LOWERCASE_LEAD_BYTE)
        && text.contains(MULTI_CHAR_LOWERCASE))
    .then(|| text.replace(MULTI_CHAR_LOWERCASE, MULTI_CHAR_LOWERCASE_EXPANSION))
}

/// Upper bound on the character comparisons a naive case-folded substring scan may do
/// before [`find_folded_index`] takes over.
///
/// The naive scan is `O(text * pattern)`: a query of 20,000 `a`s against a candidate of
/// 40,000 `a`s used to cost hundreds of milliseconds per record. Capping its work keeps
/// short candidates on the cheap path while making the worst case linear.
const NAIVE_FOLDED_SCAN_BUDGET: usize = 4096;

/// Returns whether a naive case-folded scan of these lengths stays inside the budget.
fn naive_folded_scan_affordable(text_len: usize, pattern_len: usize) -> bool {
    (text_len.saturating_sub(pattern_len) + 1).saturating_mul(pattern_len)
        <= NAIVE_FOLDED_SCAN_BUDGET
}

thread_local! {
    /// Buffer holding a folded text followed by a folded pattern, reused across calls so
    /// that [`find_folded_index`] allocates once per thread rather than once per call.
    static FOLD_SCRATCH: std::cell::Cell<String> = const { std::cell::Cell::new(String::new()) };
}

/// Largest scratch buffer kept between calls; one huge candidate must not pin memory.
const FOLD_SCRATCH_RETAINED_BYTES: usize = 64 * 1024;

/// Returns the index of the first occurrence of `pattern` in `text`, where both iterators
/// yield already-folded characters.
///
/// Copies both sides into a reusable buffer and defers to [`str::find`], whose two-way
/// search is linear in both lengths. Because [`fold_char`] maps one character to exactly
/// one character, the returned index counts characters of the caller's unfolded text.
fn find_folded_index(
    text: impl Iterator<Item = char>,
    pattern: impl Iterator<Item = char>,
) -> Option<usize> {
    let mut scratch = FOLD_SCRATCH.take();
    scratch.clear();
    scratch.extend(text);
    let split = scratch.len();
    scratch.extend(pattern);

    let (text, pattern) = scratch.split_at(split);
    let found = text
        .find(pattern)
        .map(|offset| text[..offset].chars().count());

    if scratch.capacity() > FOLD_SCRATCH_RETAINED_BYTES {
        scratch.shrink_to(FOLD_SCRATCH_RETAINED_BYTES);
    }
    FOLD_SCRATCH.set(scratch);
    found
}

/// Returns [`BONUS_CASE_EXACT`] when a completed case-insensitive match spelled every
/// matched character the way the query spelled it.
///
/// `CASE_SENSITIVE` matches are exact by construction, so they collect nothing and their
/// scores stay exactly what they were.
fn case_exact_bonus<const CASE_SENSITIVE: bool>(case_exact: bool) -> i64 {
    if !CASE_SENSITIVE && case_exact {
        BONUS_CASE_EXACT
    } else {
        0
    }
}

/// Folds one ASCII byte for comparison, leaving it as written when case matters.
fn fold_ascii<const CASE_SENSITIVE: bool>(byte: u8) -> u8 {
    if CASE_SENSITIVE {
        byte
    } else {
        byte.to_ascii_lowercase()
    }
}

fn score_unicode_text<const CASE_SENSITIVE: bool>(pattern: &str, text: &str) -> Option<i64> {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let compact_score = compact_char_match_score::<CASE_SENSITIVE>(&pattern_chars, &text_chars)?;

    let exact_bonus = if CASE_SENSITIVE {
        whole_text_bonus(pattern, text)
    } else {
        folded_whole_text_bonus(&pattern_chars, &text_chars)
    };

    Some(exact_bonus + compact_score)
}

/// Returns the identical, prefix, and substring bonus for an as-written comparison.
fn whole_text_bonus(pattern: &str, text: &str) -> i64 {
    if pattern == text {
        10_000
    } else if text.starts_with(pattern) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    }
}

/// Returns the identical, prefix, and substring bonus for a case-folded comparison.
fn folded_whole_text_bonus(pattern: &[char], text: &[char]) -> i64 {
    if folded_chars_eq(pattern, text) {
        10_000
    } else if text.len() >= pattern.len() && folded_chars_eq(pattern, &text[..pattern.len()]) {
        8_000
    } else if folded_chars_contain(text, pattern) {
        6_000
    } else {
        0
    }
}

fn folded_chars_eq(left: &[char], right: &[char]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| fold_char::<false>(*left) == fold_char::<false>(*right))
}

fn folded_chars_contain(text: &[char], pattern: &[char]) -> bool {
    let Some(last_start) = text.len().checked_sub(pattern.len()) else {
        return false;
    };

    if !naive_folded_scan_affordable(text.len(), pattern.len()) {
        return find_folded_index(
            text.iter().copied().map(fold_char::<false>),
            pattern.iter().copied().map(fold_char::<false>),
        )
        .is_some();
    }

    (0..=last_start).any(|start| folded_chars_eq(pattern, &text[start..start + pattern.len()]))
}

fn compact_char_match_score<const CASE_SENSITIVE: bool>(
    pattern: &[char],
    text: &[char],
) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }

    let mut pattern_index = 0usize;
    let mut wanted = fold_char::<CASE_SENSITIVE>(pattern[0]);
    let mut end = None;
    for (text_index, &text_ch) in text.iter().enumerate() {
        if fold_char::<CASE_SENSITIVE>(text_ch) == wanted {
            pattern_index += 1;
            if pattern_index == pattern.len() {
                end = Some(text_index);
                break;
            }
            wanted = fold_char::<CASE_SENSITIVE>(pattern[pattern_index]);
        }
    }

    let mut text_index = end?;
    let mut score = 1000;
    let mut right_match: Option<usize> = None;
    let mut first = 0usize;
    let mut case_exact = true;
    for pattern_index in (0..pattern.len()).rev() {
        let wanted = fold_char::<CASE_SENSITIVE>(pattern[pattern_index]);
        while fold_char::<CASE_SENSITIVE>(text[text_index]) != wanted {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
        let position = text_index;
        first = position;
        if !CASE_SENSITIVE && text[position] != pattern[pattern_index] {
            case_exact = false;
        }

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
        score + case_exact_bonus::<CASE_SENSITIVE>(case_exact)
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

fn score_ascii_text<const CASE_SENSITIVE: bool>(pattern: &str, text: &str) -> Option<i64> {
    let pattern_bytes = pattern.as_bytes();
    let text_bytes = text.as_bytes();

    let compact_score = compact_ascii_match_score::<CASE_SENSITIVE>(pattern_bytes, text_bytes)?;

    let exact_bonus = if CASE_SENSITIVE {
        whole_text_bonus(pattern, text)
    } else {
        folded_ascii_whole_text_bonus(pattern_bytes, text_bytes)
    };

    Some(exact_bonus + compact_score)
}

/// Returns the identical, prefix, and substring bonus for a case-folded ASCII comparison.
fn folded_ascii_whole_text_bonus(pattern: &[u8], text: &[u8]) -> i64 {
    if text.eq_ignore_ascii_case(pattern) {
        10_000
    } else if text.len() >= pattern.len() && text[..pattern.len()].eq_ignore_ascii_case(pattern) {
        8_000
    } else if find_ascii_ignore_case(text, pattern).is_some() {
        6_000
    } else {
        0
    }
}

/// Returns the byte offset of the first case-folded occurrence of `pattern` in `text`.
///
/// Both slices must be ASCII, so a byte offset is also a character index.
fn find_ascii_ignore_case(text: &[u8], pattern: &[u8]) -> Option<usize> {
    debug_assert!(text.is_ascii() && pattern.is_ascii());
    let Some((&first, rest)) = pattern.split_first() else {
        return Some(0);
    };
    let first = first.to_ascii_lowercase();
    let last_start = text.len().checked_sub(pattern.len())?;

    if rest.is_empty() {
        return text
            .iter()
            .position(|byte| byte.to_ascii_lowercase() == first);
    }

    if !naive_folded_scan_affordable(text.len(), pattern.len()) {
        return find_folded_index(
            text.iter()
                .map(|byte| char::from(byte.to_ascii_lowercase())),
            pattern
                .iter()
                .map(|byte| char::from(byte.to_ascii_lowercase())),
        );
    }

    (0..=last_start).find(|&start| {
        text[start].to_ascii_lowercase() == first
            && text[start + 1..start + pattern.len()].eq_ignore_ascii_case(rest)
    })
}

fn compact_ascii_match_score<const CASE_SENSITIVE: bool>(
    pattern: &[u8],
    text: &[u8],
) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }

    let mut pattern_index = 0usize;
    let mut wanted = fold_ascii::<CASE_SENSITIVE>(pattern[0]);
    let mut end = None;
    for (text_index, &text_byte) in text.iter().enumerate() {
        if fold_ascii::<CASE_SENSITIVE>(text_byte) == wanted {
            pattern_index += 1;
            if pattern_index == pattern.len() {
                end = Some(text_index);
                break;
            }
            wanted = fold_ascii::<CASE_SENSITIVE>(pattern[pattern_index]);
        }
    }

    let mut text_index = end?;
    let mut score = 1000;
    let mut right_match: Option<usize> = None;
    let mut first = 0usize;
    let mut case_exact = true;
    for pattern_index in (0..pattern.len()).rev() {
        let wanted = fold_ascii::<CASE_SENSITIVE>(pattern[pattern_index]);
        while fold_ascii::<CASE_SENSITIVE>(text[text_index]) != wanted {
            if text_index == 0 {
                return None;
            }
            text_index -= 1;
        }
        let position = text_index;
        first = position;
        if !CASE_SENSITIVE && text[position] != pattern[pattern_index] {
            case_exact = false;
        }

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
        score + case_exact_bonus::<CASE_SENSITIVE>(case_exact)
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
///
/// With `case_sensitive` false the substring search itself is case-folded, and an occurrence
/// spelled exactly like the pattern collects [`BONUS_CASE_EXACT`].
pub fn score_exact_text(pattern: &str, text: &str, case_sensitive: bool) -> Option<i64> {
    if let Some(score) = score_exact_folded_text(pattern, text, case_sensitive) {
        return Some(score);
    }
    if case_sensitive {
        return None;
    }

    retry_with_lowercase_expansion(pattern, text, |pattern, text| {
        score_exact_folded_text(pattern, text, false)
    })
}

/// Scores an exact substring match comparing both sides as written, up to a 1:1 case fold.
fn score_exact_folded_text(pattern: &str, text: &str, case_sensitive: bool) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    // A folded hit that still starts with the pattern's own bytes matched nothing by folding,
    // so it is the occurrence the user spelled. Case-sensitive hits are all of that kind and
    // collect nothing, keeping their scores exactly what they were.
    let (start, case_bonus) = if case_sensitive {
        (text.find(pattern)?, 0)
    } else {
        let start = find_ignore_case(text, pattern)?;
        (
            start,
            case_exact_bonus::<false>(text[start..].starts_with(pattern)),
        )
    };
    // Only a match at the very start can cover the whole text.
    let whole_text = start == 0
        && if case_sensitive {
            pattern == text
        } else {
            eq_ignore_case(pattern, text)
        };

    let exact_bonus = if whole_text {
        10_000
    } else if start == 0 {
        8_000
    } else {
        6_000
    };
    Some(1000 + exact_bonus + case_bonus - start as i64 * 5 - text.chars().count() as i64)
}

/// Returns the byte offset of the first case-folded occurrence of `pattern` in `text`.
fn find_ignore_case(text: &str, pattern: &str) -> Option<usize> {
    if text.is_ascii() && pattern.is_ascii() {
        return find_ascii_ignore_case(text.as_bytes(), pattern.as_bytes());
    }

    if !naive_folded_scan_affordable(text.len(), pattern.len()) {
        let char_index = find_folded_index(
            text.chars().map(fold_char::<false>),
            pattern.chars().map(fold_char::<false>),
        )?;
        return text
            .char_indices()
            .nth(char_index)
            .map(|(offset, _)| offset);
    }

    let first = fold_char::<false>(pattern.chars().next()?);
    text.char_indices()
        .filter(|&(_, ch)| fold_char::<false>(ch) == first)
        .map(|(index, _)| index)
        .find(|&index| starts_with_ignore_case(&text[index..], pattern))
}

fn starts_with_ignore_case(text: &str, pattern: &str) -> bool {
    let mut text_chars = text.chars();
    pattern.chars().all(|expected| {
        text_chars.next().map(fold_char::<false>) == Some(fold_char::<false>(expected))
    })
}

fn eq_ignore_case(left: &str, right: &str) -> bool {
    left.chars()
        .map(fold_char::<false>)
        .eq(right.chars().map(fold_char::<false>))
}

fn is_boundary(text: &[char], position: usize) -> bool {
    position == 0 || matches!(text[position - 1], '/' | '\\' | '_' | '-' | ' ' | '.')
}

fn is_path_or_field_delimiter(ch: char) -> bool {
    matches!(ch, '/' | '\\' | ',' | ':' | ';' | '|')
}

#[cfg(test)]
mod tests;
