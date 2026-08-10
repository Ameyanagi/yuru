# Architecture And Optimization

Yuru is shaped by a problem that fzf mostly avoids: a candidate can have more
than one searchable form. A visible path such as `資料/東京駅.pdf` may need direct
matching, normalized-width matching, kana readings, romaji readings, source-span
highlighting, Korean Hangul romanization, choseong initials, and shell-friendly
path ranking at the same time.

## Agentic Coding

Yuru was developed with heavy AI assistance. The project direction, feature
choices, language behavior, testing decisions, release process, and maintenance
are human-led by the maintainer. AI tools were used extensively during
implementation and documentation, but the code is reviewed, tested, and
maintained as an open-source project rather than published as unreviewed AI
output.

## Multilingual Matching

Multilingual fuzzy finding adds a few constraints beyond plain ASCII matching:

- Candidate text is indexed into multiple search keys: original text,
  normalized text, and language-specific keys such as Japanese kana/romaji or
  Korean Hangul romanization/initials or Chinese pinyin/initials.
- Query text is expanded into query variants, then each variant is allowed to
  match only compatible key kinds. This prevents accidental cross-language
  matches while still allowing `kamera` to match `カメラ` or `bjdx` to match
  `北京大学`.
- Generated reading keys can carry source maps, so a match on a generated
  reading can highlight the original CJK surface text instead of the whole
  candidate.
- `--lang auto` chooses one backend before indexing from the query, locale, and
  currently available candidate sample. It intentionally does not build
  Japanese, Korean, and Chinese keys for every candidate at the same time.

## Indexing

Indexing is candidate-side. For each candidate Yuru builds:

- an original key
- a normalized key when normalization is enabled
- language-backend keys for Japanese, Korean, or Chinese mode
- optional learned alias keys

Case folding is a matcher concern rather than a normalization side effect, so the
matcher compares case-insensitively unless the search is case-sensitive. A
normalized key that only case-folds its candidate (the usual outcome for plain
ASCII text) is therefore flagged `case_fold_only` at index time and skipped while
scoring case-insensitively: the original key already carries the same match with a
higher weight. Normalized keys that also fold width, kana, or dashes stay active.

That skip is only sound for a matcher that folds case with the *same* mapping the
flag was computed with, so it is conditional on `MatcherBackend::folds_case`, whose
default is `false`. Yuru's own greedy and exact matchers fold with exactly that
mapping and say so. Two kinds of matcher must not: a matcher supplied by a library
embedder through `search_with_stats`, which is never told the search's case policy
and may be case-sensitive by construction, and `NucleoMatcher`, because
`nucleo-matcher` folds with its own simple-case-folding table that disagrees with
`char::to_lowercase` for 55 characters its table does not know (`Ɤ` U+A7CB, `Ᲊ`
U+1C89, the Garay block). Both are offered the folded key as well, which costs one
extra scored key per candidate on those paths and is what makes `--filter ɤ` still
find `Ɤx` under `--algo v2`.

`NucleoMatcher::folds_case` stays `false` in *both* case modes, and deliberately is
not `!case_sensitive`. The question it answers is which folding *mapping* the
matcher uses, not whether it folds at all, and nucleo's mapping is the wrong one
either way; a case-sensitive nucleo matcher folds nothing, so `false` is right there
too. The value is therefore constant while the case policy is not: `NucleoMatcher`
carries its policy in the wrapped `nucleo_matcher::Config`, built by
`NucleoMatcher::new(case_sensitive)` from `SearchConfig::case_sensitive`, so
`--algo v2 --no-ignore-case` is case-sensitive like every other algorithm.

Only the haystack is left as written for nucleo to fold; the *needle* is yuru's
responsibility. `nucleo_matcher::Matcher` documents that the needle must arrive
already case-folded, because an `ignore_case` matcher folds only the haystack and
then expects the needle to already be lower case. This is not a soft requirement: on
an all-ASCII needle and haystack, nucleo's prefilter and its scoring matrix disagree
and it panics with "should have been caught by prefilter". So an `ignore_case`
`NucleoMatcher` folds the pattern with nucleo's own `chars::to_lower_case` - its
table, not `fold_case_char`, so both sides of the comparison agree - into a buffer
reused across calls. A character nucleo's table does not know is left as written,
which is why an uppercase `Ɤ` query still reaches `Ɤx`.

Folding inside the matcher removed a preference that used to fall out of those key
weights. A candidate spelled the way the query was typed matched through the
original key (weight 3000), while a differently cased one could only match through
the normalized key (weight 2800), so the literal spelling won by 200 points. Now
that both reach the original key, a case-insensitive match that folded nothing
collects an explicit 75-point exact-case bonus instead. It is awarded once per
match rather than per matched character, so it cannot grow with query length, and
it sits between the camel-case bonus (70) and the word-boundary bonus (80): it
breaks a tie in favour of the literal spelling without overriding a genuinely
better match position. Case-sensitive searches never collect it, because every
match they make is already exact. Fuzzy matching, single-term `--exact`, and
extended exact terms (`'term`, `^term`, `term$`, `^term$`, `'term'`) all award it on
the same terms; the extended path has to work harder to know it applies, which the
search-path section below describes.

The matcher's folding is one character in, one character out, because every
comparison is pairwise and every reported position indexes the unfolded text. A
character whose full lowercase mapping is longer than one character - `İ`
(U+0130) lowercases to `i` plus U+0307 COMBINING DOT ABOVE - is therefore
compared as written rather than folded, since folding to just the `i` would make
`İ` match a pattern containing a bare `i`, which it does not: the combining dot
sits between them, so `--exact ia` must not match `İa`.

Comparing it as written is not the same as not folding it. When a case-insensitive
comparison finds nothing, `score_text` and `score_exact_text` retry it once with
that mapping written out on *copies* of both sides. The copies expand alike, every
remaining fold is 1:1 again, and the retry can only add matches: it is reached only
where the as-written comparison already answered `None`, so nothing that matched
before is re-scored. Doing it as a retry rather than as a pre-pass is also what
keeps it off the hot path - the gate is a `memchr` for the character's UTF-8 lead
byte, `0xC4`, chosen over its trailing `0xB0` because that byte is an ordinary
continuation byte and searching for it cost CJK searches 3-7%.

Only entry points that return nothing but a score may use the retry, because the
indices the expanded comparison computes are indices into the expanded copies.
`match_positions`, which does report indices into its argument, handles the
expansion itself, carrying each character's unexpanded index alongside it. The
extended path never needed the retry: `comparable` folds with `str::to_lowercase`,
which writes the mapping out already, and the flags that guard its position claims
(`literal_folds_to_needle`, `ExactHaystack::folds_key_case`) go false when it does.
U+0130 is the only character this concerns - an exhaustive walk of
`char::to_lowercase` finds no second one, and a unit test pins that so a Unicode
table update cannot quietly add one.

Before the retry existed, these characters were reachable case-insensitively only
through the normalized key, whose text carries the written-out expansion - so
`--literal`, which builds no normalized key, turned `--ignore-case` back off for
them. `--algo v2` / `--algo nucleo` still has that gap under `--literal`, since
nucleo folds with its own 1:1 table and yuru does not pre-fold the haystack for it.

Case-folded substring search cannot reuse `str::find`, which compares as written.
A naive folded scan is `O(text * pattern)`, so it runs only while its comparison
count stays inside a fixed budget; past that the matcher folds both sides into a
per-thread scratch buffer once and defers to `str::find`, whose two-way search is
linear. Short candidates keep the allocation-free path, and a repetitive query
against long records can no longer stall the search worker.

Generated and other non-base search keys are deduplicated and capped by both key
count and total key bytes. Required base keys such as original and normalized are
kept even when those caps are reached, so base-key and display storage still
scale with candidate length. Large batch indexes are parallelized with Rayon,
while interactive streaming mode builds candidate keys incrementally as records
arrive from stdin or the default command.

### Index Complexity

Let:

- `N` be the number of candidates
- `L` be the average visible candidate length in characters
- `K` be the number of generated search keys per candidate after capping
- `B` be the total generated key bytes per candidate after capping

Plain indexing is `O(N * L)` for original and normalized keys. Memory is
`O(N * L)` for display/base-key storage plus `O(N * B)` for capped non-base
keys. The generated-key part is bounded in practice by
`max_search_keys_per_candidate` and `max_total_key_bytes_per_candidate`.

Language backends add candidate-side work:

- Japanese kana-only keys are linear in candidate length. Lindera kanji reading
  generation has tokenizer/dictionary cost and is the heaviest language path.
- Korean Hangul keys are linear in the number of Hangul syllables. Each syllable
  is decomposed by Unicode arithmetic and contributes to romanized, initials,
  and keyboard-layout keys.
- Chinese pinyin keys are linear in the number of Han characters handled by the
  pinyin backend. `zh.polyphone = "none"` emits primary full/joined/initial
  keys. `zh.polyphone = "common"` adds capped single-character heteronym
  substitutions, still emitted as full, joined, and initials keys; it does not
  build the full Cartesian product of every possible reading.

The important design choice is that expensive language work happens at indexing
time, not for every query. Search then operates on already-built keys.

## Searching

Search is query-side. On each query change Yuru expands the query into a small,
deduplicated set of variants, scores only compatible variant/key pairs, and
keeps the best key per candidate. Ranking then applies score plus configured
tiebreaks such as length, pathname, begin/end position, and original input
index.

The hot path has a few guardrails:

- `max_query_variants`, `max_search_keys_per_candidate`, and
  `max_total_key_bytes_per_candidate` bound combinatorial growth.
- Large searches can run in parallel chunks.
- Sorted searches with `1 <= limit <= STREAMING_TOP_RESULTS_LIMIT` use a
  top-results path instead of keeping every match.
- Larger result sets use partial selection before final sorting.
- `--no-sort` restores matches to input order before truncation.
- The TUI runs search work on a background worker and ignores stale responses
  using request sequence numbers. Each response is also tagged with the search it
  answers - the query text plus the case policy that text resolved to, since smart
  case makes the policy follow the query - and only a result set whose tag equals the
  live one may become an outcome. Stale rows are still painted while a search runs,
  but an acceptance that arrives against them is held until the live search lands.

### Standard Search Path

The default matcher path is the standard search path in `yuru-core::rank`.
This is used for the normal `--algo greedy` mode and for the compatibility
alias `--algo fzf-v1`. Exact mode also uses the same standard path when the
query does not require fzf-style extended parsing, with the score function
switched from fuzzy subsequence scoring to exact substring scoring.

`standard` here means that Yuru does not call a boxed matcher backend for every
candidate/key pair. Instead, it expands the query once, scans the already-built
candidate keys directly, calls `score_text` or `score_exact_text` with the
configured case policy, and keeps only the best compatible key for each candidate. This path is the one that can
parallelize large candidate sets with Rayon chunks and use the bounded
`TopResults` buffer for small sorted limits.

Yuru leaves this standard path when the query needs extended fzf syntax, when
filtering is disabled, or when `--algo fzf-v2` / `--algo nucleo` selects the
nucleo-backed quality matcher. Normal nucleo-backed searches use a concrete
Nucleo path with one matcher per Rayon chunk on large inputs.

Extended syntax prepares the query once per search rather than once per
candidate. Parsing, each term's query-variant expansion, and each term's
case-folded comparison needle are all hoisted out of the candidate loop, and
exact terms reuse the candidate's existing normalized key instead of re-folding
its text per candidate. Like the standard path, extended search parallelizes
large candidate sets with Rayon chunks. Caller-owned matcher paths still go
through `search_with_stats` and a mutable matcher backend.

That reuse is why an extended exact term earns its exact-case bonus differently
from the standard path. `score_exact_text` folds while it searches, so it can read
case-exactness straight off the match it found (`text[start..]` still starts with
the pattern's own bytes). An extended term instead compares two texts that were
folded before it ran, which is what makes the reuse cheap - re-normalizing every
key per candidate costs about 0.5s on 500k lines - and which throws that
information away. The bonus is therefore recovered by re-checking the *located*
occurrence in the key text as written: the match's offset is translated into the
original text and the term as the user typed it must start there. Checking for a
literal occurrence anywhere instead would credit a spelling the score was not
computed from, which is not what the standard path rewards.

Translating that offset needs the folded haystack and the key text to hold the same
characters in the same positions, which is true of case folding and not of
normalization in general: NFKC can split one character into several. Both sides
must therefore prove it before a bonus is possible. The candidate side reuses the
`case_fold_only` flag the index already computed; the query side compares the
term's needle against the term as typed once per search. When either fails - a
full-width or kana term, or a candidate whose normalized key really is normalized -
the term matches exactly as before and takes no bonus, which is also the honest
answer: text that had to be normalized to match was not spelled the way the query
was typed. Positions are compared in characters rather than bytes, because folding
can resize a character (`U+212A` KELVIN SIGN folds to a one-byte `k`); pure-ASCII
pairs skip that count, since ASCII folding leaves every offset alone.

### Search Complexity

Let:

- `V` be the number of query variants after `max_query_variants`
- `K` be the number of keys on a candidate after key caps
- `Lk` be the average searchable key length
- `Q` be query length
- `M` be the number of matched candidates
- `R` be the requested result limit

The standard greedy path scores at most `N * V * K` compatible pairs. Yuru's
default matcher performs a forward subsequence scan and a backward compaction
pass, so each score is `O(Lk + Q)` and the scan is approximately
`O(N * V * K * (Lk + Q))`. Because `Q <= Lk` for successful fuzzy matches and
because `V` and `K` are capped small values, the practical shape is close to
linear in candidate count and key length.

Extended queries add a factor for the term count `T`: a group's terms are checked
against the candidate's keys until one fails, so the worst case is
`O(N * T * V * K * (Lk + Q))`, with early exit on the first unmatched non-negated
term. Because preparation is hoisted out of the candidate loop it costs
`O(T * (Q + variant fanout))` per search instead of per candidate, which is what
makes a multi-term query cost about the same per candidate as a single-term one.

Exact mode uses contiguous matching and is also linear in key length per checked
pair. Algorithm names are backend selectors rather than exact fzf
reimplementations: `--algo fzf-v1` uses the same Yuru greedy scorer as
`--algo greedy`, while `--algo fzf-v2` and `--algo nucleo` use the
`nucleo-matcher` quality path. Normal nucleo-backed searches parallelize large
candidate sets with one mutable matcher per chunk. They still do more work per
checked pair than the greedy scorer, so use the default greedy path when
predictable latency is more important than best alignment quality.

Ranking cost depends on result handling:

- `--no-sort` restores input order before truncation, so result finalization is
  `O(M log M)` today because it sorts matched IDs.
- Sorted searches with `1 <= R <= 1024` use a bounded top-results buffer backed by
  a binary heap ordered on the full rank key (score, then the configured tiebreaks
  in order, then display). The current worst kept entry is the heap root, so a
  candidate that cannot displace it costs one comparison and a replacement costs
  `O(log R)`, making finalization `O(M log R + R log R)`.
- Larger sorted result sets use partial selection followed by sorting the
  returned window, approximately `O(M + R log R)`.

Highlighting is intentionally not in the hot search loop. Search stores
`key_index`, and source-span highlighting is computed only for visible or
accepted results.

## Comparison With fzf

fzf is optimized for the general case: one input line is one searchable string,
and the matching algorithm ranks subsequence alignments within that string. Its
own source describes `FuzzyMatchV1` as a first-match greedy algorithm with
`O(n)` time, and `FuzzyMatchV2` as a modified Smith-Waterman-style algorithm
with `O(nm)` time when a match is found and `O(n)` when no match is found, where
`n` is item length and `m` is pattern length. fzf also falls back to v1 for
large inputs where the dynamic-programming matrix would be too expensive.

Yuru borrows the line-oriented filter model and fzf-style scoring ideas, but the
main implementation difference is the key model:

| Area | fzf | Yuru |
| --- | --- | --- |
| Candidate representation | one searchable item string | original, normalized, language keys, and aliases |
| Multilingual matching | mostly direct Unicode text matching | generated Japanese, Korean, and Chinese phonetic keys |
| Query expansion | fzf query terms and modes | base query variants plus language-aware variants |
| Highlighting | match positions in the visible item | source maps can project generated-key matches back to CJK text |
| Latency strategy | highly optimized matcher over the item list | bounded keys/variants, parallel search, streaming index, background workers |
| Preview strategy | external preview command model | external previews plus built-in text/image preview workers |

The tradeoff is explicit: Yuru does more work per candidate during indexing so a
single query can match forms that are not visible in the original text. The caps
on query variants, non-base key count, and generated-key bytes are there to keep
that extra expressiveness from turning into unbounded search work.

References:

- [fzf README](https://github.com/junegunn/fzf)
- [fzf matching algorithm source comments](https://github.com/junegunn/fzf/blob/master/src/algo/algo.go)

## Streaming And Lazy Work

Interactive mode can open while stdin or a default command is still producing
candidates. A source worker reads records, builds candidate keys, and appends
them to the live candidate set. The search worker reruns against the currently
available candidates when new records arrive or the query changes, so the UI can
stay responsive instead of waiting for a full source command to finish.

This is not a global persistent index. It is a session-local, lazy/streaming
index tuned for command-line workflows.

## Selection Across Result Replacements

Because the search worker replaces the result list wholesale, nothing about a
row's position is stable: the same index means a different candidate after every
landing. `TuiState` therefore keeps a `SelectionTarget`, not a row number. It is
either `Top` — nothing has been chosen since the query last changed, so the
selection follows whatever comes first — or `Row(id)`, a specific candidate. The
row index the renderer draws the cursor on is a cache of where that target
currently sits, recomputed by `TuiState::reselect` at the one place a result list
is installed. A `Row` that is missing from the replacement resets to `Top`, which
is what fzf does and the least surprising of the options.

Three separate questions have to be answered to accept the right line, and each
has exactly one mechanism:

- *Do these rows answer what is typed?* `ResultSet` carries the `SearchIdentity`
  (query text plus resolved case policy) its rows were computed for, and compares
  it against the live one. Superseded rows keep being drawn — blanking the list
  for the duration of a 500,000-candidate search would trade one defect for a
  worse one — but they are never turned into an outcome.
- *When rows are superseded, what happens to `Enter`?* It is held in a
  `PendingAccept` until the live search lands, then resolved. Only `Ctrl-C` /
  `Esc` are honoured in the meantime; anything else would retarget a decision the
  user has already made.
- *Which row was it aimed at?* `PendingAccept` captures the `SelectionTarget` as
  it was when the key was pressed. Resolving against the live selection instead
  would hand back whatever moved into that place, which is the same class of bug
  one level up. A captured `Row` that is not in the live results accepts nothing.

Marks are stored the same way, as candidate ids in the order they were marked, so
refining the query does not discard them.

## Preview

Preview work is kept off the main UI loop. The TUI stores preview state in a
`PreviewCache` keyed by preview command, shell, selected candidate id, selected
display text, and preview geometry. A key change clears the old content, resets
scroll, and schedules a debounced request. The request then runs on a worker
thread and returns either text or decoded image data.

There are two command modes:

- `--preview <command>` uses the shell preview path. Yuru expands the `{}` token,
  runs the command with fzf-compatible geometry environment variables
  (`FZF_PREVIEW_COLUMNS`, `FZF_PREVIEW_LINES`, `FZF_PREVIEW_LEFT`,
  `FZF_PREVIEW_TOP`), and treats stdout as the preview when it is text. If
  stdout is image bytes or text pointing at an image path, it becomes an image
  preview. If stdout is empty, stderr is shown; a nonzero command with no stderr
  becomes a short exit-status message.
- `--preview-auto` uses the built-in path. Directories show a sorted entry list,
  missing paths and non-text files show metadata, empty files are reported
  explicitly, and text files are rendered with `bat --style=numbers
  --color=never --paging=never --line-range :200` when available. If `bat`
  fails or is absent, Yuru falls back to bounded direct file reading. Files
  are considered text when their extension is configured as text or their first
  8192 bytes look like ASCII text.

Image preview is compiled behind the `image` feature. If the selected item
itself is an image path, that takes precedence over shell/built-in text preview.
Yuru recognizes `png`, `jpg`, `jpeg`, `gif`, `bmp`, `ico`, `tif`, `tiff`,
`webp`, `svg`, and `svgz` paths. Raster images are decoded with the `image`
crate; SVGs are rasterized with `resvg`, capped to a 2048-pixel maximum axis.
The decoded image is cached separately from terminal encoding.

Each preview command output stream and built-in text reads are capped at 1 MiB,
and preview commands are terminated after five seconds. Raster input is capped
at 32 MiB, 8192 pixels per axis, 16 megapixels, and 128 MiB of decoder
allocation. SVG preview disables external and local `href` resource loading.
Replaced previews cancel and join their workers and terminate the preview
process group.

Terminal image encoding is also asynchronous. `--preview-image-protocol none`
disables image rendering and reports compact image metadata. With
`--preview-image-protocol auto`, `YURU_PREVIEW_IMAGE_PROTOCOL` wins, then terminal
environment heuristics choose Kitty/Ghostty, iTerm2/WezTerm/Rio, or
Sixel-capable terminals. Explicit `halfblocks`, `sixel`, `kitty`, and `iterm2`
values force that protocol. If `auto` detects no protocol, the picker falls back
to half-block rendering. The image worker resizes to fit the current preview area
and re-encodes only when that area changes.

This keeps selection movement and query input responsive even when a preview
command, image decoder, or terminal image encoder is slower than the search
path.
