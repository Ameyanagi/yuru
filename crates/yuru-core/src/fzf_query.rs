use std::borrow::Cow;

use crate::{
    key_kind_allowed,
    matcher::{
        fold_case_char, BONUS_CASE_EXACT, MULTI_CHAR_LOWERCASE, MULTI_CHAR_LOWERCASE_EXPANSION,
    },
    normalize,
    query::{key_blocked_by_config, prepare_query_variants, variant_blocked_by_config},
    Candidate, KeyKind, LanguageBackend, MatcherBackend, QueryVariant, ScoredCandidate,
    SearchConfig, SearchKey, SearchStats,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExtendedQuery {
    groups: Vec<Vec<Term>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Term {
    text: String,
    negated: bool,
    mode: TermMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TermMode {
    Fuzzy,
    Exact,
    Prefix,
    Suffix,
    Equal,
    Boundary,
}

/// Extended query parsed and expanded once for a whole search run.
pub(crate) struct PreparedQuery {
    groups: Vec<Vec<PreparedTerm>>,
    variants_seen: usize,
}

/// One extended-query term with its per-run matching inputs precomputed.
struct PreparedTerm {
    negated: bool,
    mode: TermMode,
    /// Expanded query variants; empty unless `mode` is [`TermMode::Fuzzy`].
    variants: Vec<QueryVariant>,
    /// [`comparable`] form of the term text; empty when `mode` is [`TermMode::Fuzzy`].
    needle: String,
    /// The term text as the user typed it; empty when `mode` is [`TermMode::Fuzzy`].
    literal: String,
    /// True when [`Self::needle`] is [`Self::literal`] with every character folded by
    /// [`fold_case_char`], i.e. case is the only thing [`comparable`] changed about the term.
    ///
    /// Only then does an as-written comparison against a case-folded haystack mean anything:
    /// the two texts have the same characters in the same positions, so the term the user
    /// typed can be looked for at the character index the folded match was found at. A term
    /// that normalization also changed (width, kana, a dash variant) was not typed the way
    /// any candidate spells it, so it forfeits [`BONUS_CASE_EXACT`] rather than guessing.
    literal_folds_to_needle: bool,
}

impl PreparedQuery {
    /// Parses `query` and precomputes the variants and needles every candidate reuses.
    pub(crate) fn new(query: &str, backend: &dyn LanguageBackend, config: &SearchConfig) -> Self {
        let parsed = ExtendedQuery::parse(query, config.exact);
        let mut variants_seen = 0;
        let mut groups = Vec::with_capacity(parsed.groups.len());

        for group in parsed.groups {
            let mut prepared_group = Vec::with_capacity(group.len());
            for term in group {
                let prepared = PreparedTerm::new(term, backend, config);
                variants_seen += prepared.variants.len();
                prepared_group.push(prepared);
            }
            groups.push(prepared_group);
        }

        Self {
            groups,
            variants_seen,
        }
    }

    /// Returns the number of query variants expanded for this query.
    pub(crate) fn variants_seen(&self) -> usize {
        self.variants_seen
    }
}

impl PreparedTerm {
    fn new(term: Term, backend: &dyn LanguageBackend, config: &SearchConfig) -> Self {
        let fuzzy = term.mode == TermMode::Fuzzy;
        let (variants, needle, literal) = if fuzzy {
            (
                prepare_query_variants(&term.text, backend, config),
                String::new(),
                String::new(),
            )
        } else {
            (Vec::new(), comparable(&term.text, config), term.text)
        };
        let literal_folds_to_needle =
            !fuzzy && needle.chars().eq(literal.chars().map(fold_case_char));

        Self {
            negated: term.negated,
            mode: term.mode,
            variants,
            needle,
            literal,
            literal_folds_to_needle,
        }
    }
}

pub(crate) fn score_candidate<M: MatcherBackend + ?Sized>(
    query: &PreparedQuery,
    candidate: &Candidate,
    matcher: &mut M,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<ScoredCandidate> {
    if query.groups.is_empty() {
        return Some(ScoredCandidate {
            id: candidate.id,
            display: candidate.display.clone(),
            score: 0,
            key_kind: KeyKind::Original,
            key_index: 0,
        });
    }

    let mut best: Option<ScoredCandidate> = None;
    // One set of fold-replay caches per candidate: the mapping they hold is a
    // property of (key, haystack), which never changes within one candidate's
    // scoring and must never survive past it.
    let mut replays = ReplayCaches::default();
    for group in &query.groups {
        let mut group_score = 0i64;
        let mut group_kind = KeyKind::Original;
        let mut group_key_index = 0u32;
        let mut group_matches = true;

        for term in group {
            let matched = match_term(term, candidate, matcher, config, stats, &mut replays);
            if term.negated {
                if matched.is_some() {
                    group_matches = false;
                    break;
                }
                continue;
            }

            if let Some((score, kind, key_index)) = matched {
                group_score += score;
                group_kind = kind;
                group_key_index = key_index;
            } else {
                group_matches = false;
                break;
            }
        }

        if group_matches {
            let scored = ScoredCandidate {
                id: candidate.id,
                display: candidate.display.clone(),
                score: group_score,
                key_kind: group_kind,
                key_index: group_key_index,
            };
            if best
                .as_ref()
                .is_none_or(|current| scored.score > current.score)
            {
                best = Some(scored);
            }
        }
    }

    best
}

pub(crate) fn requires_extended_search(query: &str) -> bool {
    let mut escaped = false;

    for ch in query.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            ' ' | '\t' | '|' | '!' | '\'' | '^' | '$' => return true,
            _ => {}
        }
    }

    false
}

fn match_term<M: MatcherBackend + ?Sized>(
    term: &PreparedTerm,
    candidate: &Candidate,
    matcher: &mut M,
    config: &SearchConfig,
    stats: &mut SearchStats,
    replays: &mut ReplayCaches,
) -> Option<(i64, KeyKind, u32)> {
    match term.mode {
        TermMode::Fuzzy => match_fuzzy_term(term, candidate, matcher, config, stats),
        TermMode::Exact
        | TermMode::Prefix
        | TermMode::Suffix
        | TermMode::Equal
        | TermMode::Boundary => match_exact_term(term, candidate, config, replays),
    }
}

fn match_fuzzy_term<M: MatcherBackend + ?Sized>(
    term: &PreparedTerm,
    candidate: &Candidate,
    matcher: &mut M,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<(i64, KeyKind, u32)> {
    // The matcher may be caller-owned, so only it can say whether it folds case.
    let scorer_folds_case = matcher.folds_case();
    let mut best: Option<(i64, KeyKind, u32)> = None;
    for variant in &term.variants {
        if variant_blocked_by_config(variant.kind, config) {
            continue;
        }

        for (key_index, key) in candidate.keys.iter().enumerate() {
            if key_blocked_by_config(key, config, scorer_folds_case)
                || !key_kind_allowed(variant, key.kind)
            {
                continue;
            }

            stats.keys_seen += 1;
            stats.fuzzy_calls += 1;
            if let Some(base_score) = matcher.score(&variant.text, &key.text) {
                let score = base_score + i64::from(variant.weight + key.weight);
                if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
                    best = Some((score, key.kind, key_index as u32));
                }
            }
        }
    }

    best
}

fn match_exact_term(
    term: &PreparedTerm,
    candidate: &Candidate,
    config: &SearchConfig,
    replays: &mut ReplayCaches,
) -> Option<(i64, KeyKind, u32)> {
    let normalized_display = reusable_normalized_key(candidate, config);
    let mut best: Option<(i64, KeyKind, u32)> = None;

    for (key_index, key) in candidate.keys.iter().enumerate() {
        // Exact terms never reach the matcher: `exact_haystack` folds each key here, with
        // `comparable`, so this path always folds case unless the config forbids it.
        if key_blocked_by_config(key, config, !config.case_sensitive) {
            continue;
        }

        let haystack = exact_haystack(key, normalized_display, config);
        let Some(hit) = exact_score(term.mode, &term.needle, &haystack) else {
            continue;
        };
        let score = hit.score
            + i64::from(key.weight)
            + case_exact_bonus(
                term,
                &key.text,
                &haystack,
                hit.byte_start,
                config,
                replays,
                key_index,
            );
        if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
            best = Some((score, key.kind, key_index as u32));
        }
    }

    best
}

/// Returns [`BONUS_CASE_EXACT`] when a case-folded hit is spelled the way the term was typed.
///
/// The extended path folds both sides before comparing them, so unlike
/// [`crate::score_exact_text`] it cannot read the answer off the match itself; it re-checks
/// the occurrence in the key text as written. The test is the same one
/// [`crate::score_exact_text`] applies (`text[start..].starts_with(pattern)`): the occurrence
/// the folded search settled on, not merely some other occurrence, must carry the term's own
/// characters. Awarding it for a literal occurrence elsewhere would credit a spelling the
/// score was not computed from.
///
/// Both sides must be positionally comparable for that test to mean anything, but only *at
/// the occurrence*: the term as a whole, through [`PreparedTerm::literal_folds_to_needle`],
/// and the key only as far as [`key_text_from`] has to walk to name the matched character.
/// Whether the fold lines up elsewhere in the key is not this occurrence's business - a key
/// holding one character that folds to two (`İ`) used to forfeit the bonus for every term in
/// the query, including an ASCII term matching an unrelated part of the text.
///
/// Yields nothing at all under case-sensitive matching, where every hit is exact by
/// construction and scores must stay what they were.
fn case_exact_bonus(
    term: &PreparedTerm,
    key_text: &str,
    haystack: &str,
    byte_start: usize,
    config: &SearchConfig,
    replays: &mut ReplayCaches,
    key_index: usize,
) -> i64 {
    if config.case_sensitive || !term.literal_folds_to_needle {
        return 0;
    }

    let Some(as_written) = key_text_from(key_text, haystack, byte_start, replays, key_index) else {
        return 0;
    };
    if as_written.starts_with(&term.literal) {
        BONUS_CASE_EXACT
    } else {
        0
    }
}

/// Returns `key_text` from the character the haystack's `byte_start` names, or `None` when the
/// haystack is not `key_text` folded character for character up to that point.
///
/// Folding resizes characters, in both directions: `U+212A` KELVIN SIGN folds to a one-byte
/// `k`, and `İ` lowercases to the two characters `i` + `U+0307`. So the offset is replayed
/// rather than used directly - one key character at a time, each advancing by however many
/// haystack bytes that character folded to. Replaying is what makes this local: an expansion
/// *before* the occurrence is walked over like any other character, where a whole-key
/// character count would have been thrown off by it and a whole-key "folds 1:1" test would
/// have refused the offset outright.
///
/// `None` when a key character does not account for the haystack bytes at its position
/// (normalization changed more than case there), and `None` when `byte_start` lands inside a
/// character's folded form instead of at its start, since no key character is spelled there.
///
/// The walk only has to reach `byte_start`; the caller's `starts_with` covers the occurrence
/// itself. A fold that expands *inside* the match cannot pass that check, because the key
/// then spells as one character what the term spells as two.
///
/// Pure-ASCII keys skip the walk once the haystack is confirmed to be their ASCII fold, since
/// ASCII folding leaves every byte offset alone.
fn key_text_from<'a>(
    key_text: &'a str,
    haystack: &str,
    byte_start: usize,
    replays: &mut ReplayCaches,
    key_index: usize,
) -> Option<&'a str> {
    if byte_start == 0 {
        return Some(key_text);
    }
    if key_text.is_ascii()
        && haystack
            .as_bytes()
            .eq_ignore_ascii_case(key_text.as_bytes())
    {
        return key_text.get(byte_start..);
    }

    // The cache is fetched - and on first use, allocated - only here, past the
    // fast paths: an ASCII hit or a hit at offset zero never pays for a replay
    // it will not perform. Fetching it eagerly at the call site doubled the
    // allocations of a plain `^a` search over ASCII candidates.
    replays
        .for_key(key_index)
        .key_offset_at(key_text, haystack, byte_start)
        .map(|offset| &key_text[offset..])
}

/// Resumable fold replay for one key, shared by every exact term of one query.
///
/// The walk from haystack byte offsets back to key byte offsets is deterministic
/// for a given key and haystack, so it is done once and checkpointed. Each term
/// used to replay the whole prefix again, which made a query of `T` exact terms
/// against a candidate whose expansion sits before the matches cost
/// `O(prefix * T)` - the subject of issue #8. Resuming from the checkpoints
/// makes the total walk `O(prefix)` per key however many terms ask.
///
/// One instance is only ever valid for one key of one candidate; the per-scoring
/// [`ReplayCaches`] owns that scoping so a cache can never answer with another
/// key's mapping.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeyReplay {
    /// `(haystack byte offset, key byte offset)` for every character boundary
    /// the walk has passed, in order. Starts implicitly at `(0, 0)`.
    checkpoints: Vec<(usize, usize)>,
    /// Haystack offset the walk has validated up to.
    walked: usize,
    /// Key offset matching `walked`.
    key_pos: usize,
    /// Set when the walk hit a fold mismatch; every offset past `walked` is
    /// unanswerable from then on.
    failed: bool,
}

impl KeyReplay {
    /// Key byte offset whose character the haystack's `byte_start` names, or
    /// `None` when the fold does not line up character for character up to it -
    /// the same contract as the un-memoized walk, checked by
    /// `memoized_replay_agrees_with_a_fresh_walk`.
    fn key_offset_at(
        &mut self,
        key_text: &str,
        haystack: &str,
        byte_start: usize,
    ) -> Option<usize> {
        while self.walked < byte_start && !self.failed {
            let Some(ch) = key_text[self.key_pos..].chars().next() else {
                break;
            };
            let Some(rest) = haystack.get(self.walked..) else {
                self.failed = true;
                break;
            };
            let folded = fold_case_char(ch);
            if rest.starts_with(folded) {
                self.walked += folded.len_utf8();
            } else if ch == MULTI_CHAR_LOWERCASE && rest.starts_with(MULTI_CHAR_LOWERCASE_EXPANSION)
            {
                // The one character `fold_case_char` refuses to fold, because its
                // lowercase mapping is two characters. `comparable` writes it out;
                // this is where the walk absorbs the extra character instead of
                // losing count.
                self.walked += MULTI_CHAR_LOWERCASE_EXPANSION.len();
            } else {
                self.failed = true;
                break;
            }
            self.key_pos += ch.len_utf8();
            self.checkpoints.push((self.walked, self.key_pos));
            #[cfg(test)]
            REPLAY_STEPS.with(|steps| steps.set(steps.get() + 1));
        }

        if byte_start == 0 {
            return Some(0);
        }
        if byte_start > self.walked {
            // The walk stopped short: a mismatch, or the key ran out first.
            return None;
        }
        // A hit at the checkpoint means `byte_start` sits on a character boundary
        // of the fold; a miss means it landed inside a character's folded form,
        // where no key character is spelled.
        self.checkpoints
            .binary_search_by_key(&byte_start, |(haystack_offset, _)| *haystack_offset)
            .ok()
            .map(|index| self.checkpoints[index].1)
    }
}

/// Fold-replay caches for the candidate currently being scored, one slot per
/// key, created lazily so the common case - keys that never produce an exact
/// hit off the fast paths - allocates nothing.
#[derive(Debug, Default)]
pub(crate) struct ReplayCaches {
    per_key: Vec<Option<KeyReplay>>,
}

impl ReplayCaches {
    fn for_key(&mut self, key_index: usize) -> &mut KeyReplay {
        #[cfg(test)]
        REPLAY_FETCHES.with(|fetches| fetches.set(fetches.get() + 1));
        if self.per_key.len() <= key_index {
            self.per_key.resize(key_index + 1, None);
        }
        self.per_key[key_index].get_or_insert_with(KeyReplay::default)
    }
}

#[cfg(test)]
thread_local! {
    /// Characters consumed by fold-replay walks, for the complexity test: with the
    /// cache, scoring `T` exact terms against one key walks its prefix once, not
    /// `T` times.
    pub(crate) static REPLAY_STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Cache fetches, for the fast-path test: an ASCII or offset-zero hit must
    /// resolve without touching - and so without ever allocating - a cache.
    pub(crate) static REPLAY_FETCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Returns the candidate's normalized key when its text already equals
/// [`comparable`] applied to the original display text.
///
/// [`crate::build_candidate`] stores `normalize_candidate(display)` in the
/// [`KeyKind::Normalized`] key whenever `config.normalize` is set, which is
/// exactly what [`comparable`] produces for the [`KeyKind::Original`] key under
/// case-insensitive normalizing search. Returns `None` when that key cannot be
/// reused, i.e. under case-sensitive matching, with normalization disabled (the
/// key is then both absent and blocked by [`key_blocked_by_config`]), or for
/// candidates built without a normalized key.
///
/// A normalized key that [`key_blocked_by_config`] skips as case-fold-only is still
/// reused here: skipping it only avoids scoring the same match twice, while its text
/// remains the folded haystack the original key needs. That skip and this reuse rest on the
/// same equivalence, so an override of [`crate::LanguageBackend::normalize_candidate`] that
/// broke one would break the other.
fn reusable_normalized_key<'a>(
    candidate: &'a Candidate,
    config: &SearchConfig,
) -> Option<&'a SearchKey> {
    if config.case_sensitive || !config.normalize {
        return None;
    }

    candidate
        .keys
        .iter()
        .find(|key| key.kind == KeyKind::Normalized)
}

/// Returns the text exact terms are located in for one key: the key text with case folded,
/// unless the config asked for case-sensitive matching.
///
/// Whether that text lines up with the key text as written is [`key_text_from`]'s question,
/// asked per occurrence and only of the prefix that matters, so nothing is computed here for
/// the keys - the overwhelming majority - that never produce a hit.
fn exact_haystack<'a>(
    key: &'a SearchKey,
    normalized_display: Option<&'a SearchKey>,
    config: &SearchConfig,
) -> Cow<'a, str> {
    if config.case_sensitive {
        return Cow::Borrowed(&key.text);
    }

    if key.kind == KeyKind::Original {
        if let Some(normalized) = normalized_display {
            return Cow::Borrowed(&normalized.text);
        }
    }

    Cow::Owned(comparable(&key.text, config))
}

/// A located exact-term match: its score, and where in the haystack it starts.
struct ExactHit {
    score: i64,
    /// Byte offset of the matched occurrence in the haystack.
    byte_start: usize,
}

fn exact_score(mode: TermMode, needle: &str, haystack: &str) -> Option<ExactHit> {
    if needle.is_empty() {
        return Some(ExactHit {
            score: 0,
            byte_start: 0,
        });
    }

    match mode {
        TermMode::Exact => {
            let start = haystack.find(needle)?;
            Some(ExactHit {
                score: 7000 - start as i64 * 5 - haystack.chars().count() as i64,
                byte_start: start,
            })
        }
        TermMode::Prefix => haystack.starts_with(needle).then(|| ExactHit {
            score: 8500 - haystack.chars().count() as i64,
            byte_start: 0,
        }),
        TermMode::Suffix => haystack.ends_with(needle).then(|| ExactHit {
            score: 8500 - haystack.chars().count() as i64,
            byte_start: haystack.len() - needle.len(),
        }),
        TermMode::Equal => (haystack == needle).then_some(ExactHit {
            score: 10_000,
            byte_start: 0,
        }),
        TermMode::Boundary => boundary_match(needle, haystack).map(|start| ExactHit {
            score: 8000 - start as i64 * 5 - haystack.chars().count() as i64,
            byte_start: start,
        }),
        TermMode::Fuzzy => None,
    }
}

fn boundary_match(needle: &str, haystack: &str) -> Option<usize> {
    for (start, _) in haystack.match_indices(needle) {
        let end = start + needle.len();
        if is_boundary_at(haystack, start) && is_boundary_at(haystack, end) {
            return Some(start);
        }
    }
    None
}

fn is_boundary_at(text: &str, byte_index: usize) -> bool {
    if byte_index == 0 || byte_index >= text.len() {
        return true;
    }

    let prev = text[..byte_index].chars().next_back();
    let next = text[byte_index..].chars().next();
    match (prev, next) {
        (Some(left), Some(right)) => {
            (!left.is_alphanumeric() || left == '_') || (!right.is_alphanumeric() || right == '_')
        }
        _ => true,
    }
}

fn comparable(text: &str, config: &SearchConfig) -> String {
    if config.case_sensitive {
        text.to_string()
    } else if config.normalize {
        normalize::normalize(text)
    } else {
        text.to_lowercase()
    }
}

impl ExtendedQuery {
    fn parse(query: &str, exact_default: bool) -> Self {
        let tokens = split_terms(query);
        let mut groups = vec![Vec::new()];

        for token in tokens {
            if token == "|" {
                groups.push(Vec::new());
                continue;
            }

            if let Some(term) = Term::parse(&token, exact_default) {
                groups.last_mut().expect("group exists").push(term);
            }
        }

        groups.retain(|group| !group.is_empty());
        Self { groups }
    }
}

impl Term {
    fn parse(raw: &str, exact_default: bool) -> Option<Self> {
        let mut text = raw;
        let mut negated = false;

        if let Some(stripped) = text.strip_prefix('!') {
            negated = true;
            text = stripped;
        }

        if text.is_empty() {
            return None;
        }

        let mut mode = if negated || exact_default {
            TermMode::Exact
        } else {
            TermMode::Fuzzy
        };

        if let Some(stripped) = text.strip_prefix('\'') {
            text = stripped;
            mode = if exact_default {
                TermMode::Fuzzy
            } else if text.ends_with('\'') && text.len() > 1 {
                text = &text[..text.len() - 1];
                TermMode::Boundary
            } else {
                TermMode::Exact
            };
        }

        let starts_with_anchor = text.starts_with('^');
        let ends_with_anchor = text.ends_with('$') && text.len() > usize::from(starts_with_anchor);
        if starts_with_anchor {
            text = &text[1..];
        }
        if ends_with_anchor {
            text = &text[..text.len() - 1];
        }

        mode = match (starts_with_anchor, ends_with_anchor) {
            (true, true) => TermMode::Equal,
            (true, false) => TermMode::Prefix,
            (false, true) => TermMode::Suffix,
            (false, false) => mode,
        };

        (!text.is_empty()).then(|| Self {
            text: text.to_string(),
            negated,
            mode,
        })
    }
}

fn split_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in query.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            ' ' | '\t' => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            '|' => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
                terms.push("|".to_string());
            }
            _ => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        terms.push(current);
    }

    terms
}

#[cfg(test)]
mod tests;
