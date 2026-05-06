# Architecture And Optimization

Yuru is shaped by a problem that fzf mostly avoids: a candidate can have more
than one searchable form. A visible path such as `資料/東京駅.pdf` may need direct
matching, normalized-width matching, kana readings, romaji readings, source-span
highlighting, and shell-friendly path ranking at the same time.

## Agentic Coding

Yuru's direction, product decisions, and fuzzy-finder behavior are human-led.
The implementation has been written primarily with agentic coding assistance:
large parts of the Rust code, shell integrations, installers, tests, and
documentation were produced through an agentic coding workflow and then steered,
reviewed, and corrected by the project maintainer.

## Multilingual Matching

Multilingual fuzzy finding adds a few constraints beyond plain ASCII matching:

- Candidate text is indexed into multiple search keys: original text,
  normalized text, and language-specific keys such as Japanese kana/romaji or
  Chinese pinyin/initials.
- Query text is expanded into query variants, then each variant is allowed to
  match only compatible key kinds. This prevents accidental cross-language
  matches while still allowing `kamera` to match `カメラ` or `bjdx` to match
  `北京大学`.
- Generated reading keys can carry source maps, so a match on a generated
  reading can highlight the original CJK surface text instead of the whole
  candidate.
- `--lang auto` chooses one backend before indexing. It intentionally does not
  build Japanese and Chinese keys for every candidate at the same time.

## Indexing

Indexing is candidate-side. For each candidate Yuru builds:

- an original key
- a normalized key when normalization is enabled
- language-backend keys for Japanese or Chinese mode
- optional learned alias keys

The key set is deduplicated and capped by both key count and total key bytes.
The original and normalized keys are treated as base keys so ordinary fuzzy
matching remains available even when language key generation is capped. Large
batch indexes are parallelized with Rayon, while interactive streaming mode
builds candidate keys incrementally as records arrive from stdin or the default
command.

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
- Small sorted result limits use a top-results path instead of keeping every
  match.
- Larger result sets use partial selection before final sorting.
- The TUI runs search work on a background worker and ignores stale responses
  using request sequence numbers.

## Streaming And Lazy Work

Interactive mode can open while stdin or a default command is still producing
candidates. A source worker reads records, builds candidate keys, and appends
them to the live candidate set. The search worker reruns against the currently
available candidates when new records arrive or the query changes, so the UI can
stay responsive instead of waiting for a full source command to finish.

This is not a global persistent index. It is a session-local, lazy/streaming
index tuned for command-line workflows.

## Preview

Preview work is kept off the main UI loop:

- preview requests are debounced
- preview output is cached by command, selection, shell, and preview geometry
- shell preview commands run in a worker thread
- built-in preview renders images internally and uses `bat`, then `cat`, for
  configured text extensions or ASCII-looking files
- image encoding is also moved to a worker and recalculated only when the
  preview area changes

This keeps selection movement and query input responsive even when a preview
command or image encoder is slower than the search path.
