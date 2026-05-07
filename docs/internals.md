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
  using request sequence numbers.

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

Exact mode uses contiguous matching and is also linear in key length per checked
pair. Algorithm names are backend selectors rather than exact fzf
reimplementations: `--algo fzf-v1` uses the same Yuru greedy scorer as
`--algo greedy`, while `--algo fzf-v2` and `--algo nucleo` use the
`nucleo-matcher` quality path. The current nucleo-backed path owns a mutable
matcher and scans candidates sequentially, so it can scale worse than the
parallel greedy path on large inputs. Use the default greedy path when
predictable latency is more important than best alignment quality.

Ranking cost depends on result handling:

- `--no-sort` restores input order before truncation, so result finalization is
  `O(M log M)` today because it sorts matched IDs.
- Sorted searches with `1 <= R <= 1024` use a bounded top-results buffer.
  Current replacement scans that buffer, so finalization is `O(M * R + R log R)`.
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
  fails or is absent, Yuru falls back to `cat`, then direct file reading. Files
  are considered text when their extension is configured as text or their first
  8192 bytes look like ASCII text.

Image preview is compiled behind the `image` feature. If the selected item
itself is an image path, that takes precedence over shell/built-in text preview.
Yuru recognizes `png`, `jpg`, `jpeg`, `gif`, `bmp`, `ico`, `tif`, `tiff`,
`webp`, `svg`, and `svgz` paths. Raster images are decoded with the `image`
crate; SVGs are rasterized with `resvg`, capped to a 2048-pixel maximum axis.
The decoded image is cached separately from terminal encoding.

Terminal image encoding is also asynchronous. The UI chooses a `viuer` picker
from the explicit `--preview-image-protocol` / config value when set; otherwise
`YURU_PREVIEW_IMAGE_PROTOCOL` wins, then terminal environment heuristics choose
Kitty/Ghostty, iTerm2/WezTerm/Rio, or Sixel-capable terminals. If no protocol is
detected, the picker falls back to half-block rendering. The image worker
resizes to fit the current preview area and re-encodes only when that area
changes.

This keeps selection movement and query input responsive even when a preview
command, image decoder, or terminal image encoder is slower than the search
path.
