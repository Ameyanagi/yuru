# Changelog

All notable user-facing changes are tracked here.

## 0.2.3

### Added

- Added `yuru --clink`, printing a Lua integration script for
  [Clink](https://chrisant996.github.io/clink/) v1.2.46+ that binds `CTRL-T`,
  `CTRL-R`, and `ALT-C` in cmd.exe - a shell fzf does not integrate with.
  Candidates come from Yuru's own walker, so no external `fd` is required, and
  history follows the same contract as the other shells: newest first,
  deduplicated keeping the newest copy, ranked by match quality with recency as
  the tiebreak. Verified on Windows 11 with Clink 1.9.31: the script loads
  without errors and `CTRL-T` opens the picker; typing into the picker could
  not be exercised over the remote test rig, so treat the first release as
  needing real-terminal feedback.

### Fixed

- Fixed `CTRL-T` / `ALT-C` breaking keyboard input on Windows under Git Bash and
  MSYS2: arrow keys and typed characters appeared as raw escape sequences instead
  of reaching the finder. A shell left alive as the pipe writer competes with a
  native console application for console input records, so on MSYS2 and Cygwin
  (`$OSTYPE` `msys*` / `cygwin*`) the bash and zsh integrations now buffer the
  candidate command's output before opening the finder, leaving nothing sharing
  the console while it is interactive. Unix keeps streaming candidates unchanged.
  Reported with the diagnosis and the fix shape by
  [@MapleLuz](https://github.com/MapleLuz) ([#11](https://github.com/Ameyanagi/yuru/issues/11));
  verified on Windows 11 Git Bash.

## 0.2.2

### Fixed

- Fixed a colour belonging to clipped-off content bleeding into the selected row's
  padding for `yuru-tui` embedders. An SGR sequence sitting exactly at the clip
  point was always retained, on the reasoning that dropping a trailing *reset*
  there would let the retained text's styling leak into the rest of the interface -
  but the rule never distinguished a reset from an opener, so a sequence that had
  been styling the discarded character survived and painted the row padding with
  its colour. A boundary sequence is now kept only when every one of its parameters
  clears styling: `0` (however written), `22`-`25`, `27`-`29`, `39`, and `49`. A
  mixed sequence such as `ESC[0;31m` still ends with red active, so it is dropped,
  and the extended colour introducers `38`/`48` classify as setting without their
  arguments - which can spell `39` or `49` - being misread as resets. Not reachable
  from the `yuru` command line, whose `--ansi` handling strips escapes before
  indexing. (#9)

### Changed

- Improved multi-term exact-query performance against candidates containing `İ`
  (U+0130), the one character whose lowercase mapping is two characters. Locating
  each exact-case match replayed the candidate's fold from the start once per
  term, making a query of many exact terms cost prefix-length times term-count;
  the replay is now checkpointed per key and shared by every term of the query.
  The issue's pathological case - 100,000 long records, eight exact terms -
  returns to its pre-0.2.1 time. Ranking output is unchanged: the memoized walk
  is tested character-for-character against the walk it replaced, and the
  differential harness reports byte-identical output against 0.2.1 across all
  257 ordered and 237 set-membership cases. (#8)

## 0.2.1

### Fixed

- Fixed an extended-query exact term (`'foo`, `--exact`) withholding its
  exact-case bonus from every candidate containing `İ` (U+0130), the one
  character whose lowercase mapping is two characters. The bonus asks whether the
  matched occurrence is spelled the way the term was typed, which needs the
  candidate's case folding to line up character for character - but that was
  tested across the whole candidate rather than at the occurrence, so one such
  character cost the candidate 75 points on *every* term in the query, including
  a pure-ASCII term matching a part of the text it was nowhere near.
  `İ a` scored 86 below `X a` for the term `'a`; it now scores like the same text
  spelled `i` + U+0307, and `İ a` ranks above `i̇ a` for a query typed `İ a` on
  every matcher path, as `--no-extended` already did. Candidates whose folding
  differs by more than case *before* the match are likewise no longer penalized
  past it.

- Fixed `CTRL-R` history search preferring the OLDEST of two equally good matches on
  zsh and fish, and showing duplicate commands. Every shell now hands the same thing to
  Yuru - newest first, with duplicates removed keeping the newest copy - and none of them
  passes `--tac` any more.

  The shells disagreed about both. zsh's `fc -rl` and fish's `history` emit newest first;
  bash's `history` and PowerShell's emit oldest first. `--tac` reversed whichever it was
  given, and since the ids it reassigns are what `--scheme history` breaks ties on,
  recency came out backwards on the two shells that were already newest-first. Only
  PowerShell deduplicated. Searching `sudo mount` on zsh therefore surfaced the oldest
  matching invocation first, with `/bin/python ...` repeated six times alongside it.

  Re-run `yuru configure`, or re-evaluate `yuru --bash` / `--zsh` / `--fish` /
  `--powershell`, to pick up the corrected binding.

- Fixed `CTRL-R` history search ranking by recency alone instead of by how well
  each entry matched. The generated bash, zsh, fish, and PowerShell integrations
  passed `--no-sort`, which returns every match in input order and never consults
  the score, so a recently typed long command whose letters merely appeared
  scattered across it outranked an exact match. Searching `sudo mount` could put
  an unrelated recent line above `sudo mount /dev/sdc1 /mnt/sdcard`.

  `--scheme history`, which the integrations already pass, makes recency the
  tiebreak - so equally good matches still come back newest first, which is the
  intended behavior. `--no-sort` on top of that was discarding the "equally good"
  part. fzf's own history widget sorts by score for the same reason and offers
  recency-only order as a toggle rather than as the default.

  Re-run `yuru configure`, or re-evaluate `yuru --bash` / `--zsh` / `--fish` /
  `--powershell`, to pick up the corrected binding.

## 0.2.0

### Breaking

These affect code that depends on the `yuru-core` or `yuru-tui` libraries. The
`yuru` command-line interface is unchanged.

- `yuru-core`: `matcher::score_text`, `score_exact_text`, and `score_key` take an
  additional `case_sensitive: bool` argument, because case folding moved into the
  matcher.
- `yuru-core`: `GreedyMatcher` and `ExactMatcher` are no longer unit structs; each
  carries `pub case_sensitive: bool`. Replace the value expression `GreedyMatcher`
  with `GreedyMatcher::default()` (case-insensitive) or `GreedyMatcher::new(flag)`.
- `yuru-core`: `SearchKey` gains the public field `case_fold_only: bool`, which
  breaks exhaustive struct literals. The `SearchKey::original` / `normalized` /
  other named constructors are unaffected and default it to `false`.
- `yuru-core`: `MatcherBackend` gains the method `folds_case`, which reports whether
  the matcher case-folds the text it scores using the same one-character-to-one-character
  lowercase mapping the index uses. It defaults to `false`, so existing
  implementations keep compiling and keep being offered every search key. Override
  it (`!case_sensitive`) only if the matcher really folds with that mapping; search
  then skips the redundant case-folded key.
- `yuru-core`: a `LanguageBackend` that overrides `normalize_candidate` must now
  keep it equivalent to `normalize::normalize` for case- and width-insensitive
  comparison, because extended-query exact matching reuses the resulting
  normalized key instead of re-folding candidate text. Source-compatible, but a
  new obligation on downstream backends.
- `yuru-tui`: `TuiOptions` gains the public field `smart_case: bool`, which breaks
  exhaustive struct literals. `..TuiOptions::default()` construction is
  unaffected and preserves the previous fixed-case behavior.
- `yuru-tui`: `TuiState::apply` takes the result list (`&[ScoredCandidate]`) where it
  took a `result_len: usize`. The selection is now bound to the id of the candidate it
  is on, so every move has to see the rows, not just how many there are. Callers that
  passed a length pass the slice they took it from.
- `yuru-tui`: `TuiState::marked` returns `&[usize]` (the marked ids in the order they
  were marked) instead of `&HashSet<usize>`. Use the new `TuiState::is_marked(id)` for
  membership.
- `yuru-tui`: new public `SelectionTarget`, returned by `TuiState::target`. It is either
  `Top` (nothing chosen since the query last changed; follow the top of the results) or
  `Row(id)` (a specific candidate).

### Changed

- **Case-insensitive matching now prefers the candidate spelled the way you typed
  the query.** Because the matcher folds case itself (see Fixed, below) instead of
  matching through a lowercased key, a case-insensitive query is scored against the
  candidate's original text, which still carries its word-boundary and camel-case
  bonuses. That alone would have reordered every mixed-case result, since the old
  preference for the literal spelling was an accident of key weights. A match that
  folded nothing now collects an explicit exact-case bonus instead: 75 points,
  between the camel-case bonus (70) and the word-boundary bonus (80), awarded once
  per match and never per character, and inert for case-sensitive searches. It
  applies to fuzzy and exact (`--exact`, `'term`) matching alike.

  Net effect for query `readme` over `readme.md`, `README.md`, `ReadMe.md`, and
  `readme_old.md`: the literal `readme.md` ranks first as it did before, and the one
  change from the previous release is that `ReadMe.md` now outranks `README.md`,
  because matching against the original text lets it keep its camel-case bonus.

  Verified scope, comparing 257 command invocations against the previous release:
  229 are byte-identical and 28 differ. All 28 are on a deliberately mixed-case
  corpus and all 28 keep the exit code. 26 of the 28 are pure reorderings with an
  identical matching set, and are permutations once `--limit` is removed; 20 of
  those 26 are extended exact terms (`'readme`, `^src 'main`, `--exact` with
  several terms), which reached the bonus later than the rest - see the Fixed entry
  below. The remaining 2 are the `--explain` cases, and they are not reorderings:
  besides the `score:` they print, they change `matched key:` and `key text:`,
  because a case-insensitive match is now reported against the candidate's original
  text instead of the lowercased normalized key. For `--explain --filter read` over
  that corpus, 2770 of 4455 result blocks move from `matched key: Normalized` to
  `matched key: Original`, and 2921 `key text:` lines change from lowercased text to
  the key as written. Case-sensitive matching (`--no-ignore-case`, or an uppercase
  query under default smart case) and all Japanese, Korean, and Chinese phonetic
  matching produce byte-identical output. Where `--limit` truncates, a reordering
  can change which candidates fall inside the limit.
  Case-*sensitive* `--algo fzf-v2` / `--algo nucleo` does change, for the separate
  reason recorded under Fixed: it never applied the case policy at all before.

  Two limits of that comparison, both found after the release went out, so read
  "28 differ" as a floor rather than the full extent. First, every one of the 257
  invocations pins `--limit`, so it compares an ordered top-N and cannot see a
  change confined below the limit. Rerunning the case-flag family unbounded shows
  11 invocations whose *matching set* grew - `--filter ABC --ignore-case --literal`
  goes from 1355 to 3973 matched lines - which is the `--ignore-case` / `--literal`
  fix recorded below finding matches 0.1.11 missed, not a reordering. Second, every
  `--algo` invocation in the set uses a lowercase query, so the set says nothing
  about `--algo fzf-v2` / `--algo nucleo` under an uppercase one; see the two
  `--algo` entries under Fixed for what actually changes there. `scripts/qa/diffout.py`
  now runs an unbounded second pass covering both gaps.
- Improved multi-term query performance substantially. Extended fzf-style queries
  parsed the query, re-expanded every term's query variants, and re-normalized
  every candidate key once *per candidate*; all of that is now done once per
  search, and exact terms reuse the candidate's existing normalized key. Extended
  search also parallelizes large candidate sets with Rayon like the standard path
  does. On 500k lines, `--filter 'ab cd'` went from 0.79s to 0.25s, which is now
  the same cost as the equivalent `--no-extended` search - adding a space to a
  query is no longer a 3x penalty. Per-term scaling improved as well:
  `--filter 'ab cd ef gh'` went from 0.86s to 0.26s, so four terms now cost about
  what two did.
- Improved sorted-result selection for limits of 1024 or fewer. The bounded
  top-results buffer replaced two linear scans per scored candidate with a binary
  heap keyed on the full rank, so selection is now `O(log limit)` per candidate
  instead of `O(limit)`. On 500k lines an unfiltered `--limit 1000` went from
  0.78s to 0.32s; previously that path was 2.4x *slower* than not bounding results
  at all, which defeated its purpose. Ranking output is unchanged, including
  tiebreak-aware eviction when scores tie.
- Improved indexed search throughput generally, as a consequence of the two changes
  above: on the project's own benchmarks, plain search over 100k candidates is
  0.64x its previous time (0.90x with `--algo nucleo`, which keeps scoring the
  case-folded key because nucleo does not fold case the way the index does),
  Japanese search over 100k is 0.76x, and Chinese search over 100k is 0.71x. Index
  build time is unchanged, so none of this is paid for at indexing time.
- Improved `--delimiter` performance: the delimiter regex is now compiled once at startup instead of once per input line per field expression (500k lines with `--nth 2 -d /`: 0.95s to 0.34s). An invalid `--delimiter` pattern is now reported at startup even when no field expression is used.

### Fixed

- Fixed East Asian and emoji rows corrupting the interactive layout: result rows, the prompt, headers, footers, the preview pane, and the selected-row background are now budgeted by terminal display columns instead of character counts, so a wide row no longer wraps. A wide character that does not fit in the remaining columns is dropped whole rather than split, and the `--ellipsis` string is charged in columns too (`..` costs two, `…` costs one).
- Fixed accented and modified characters losing their trailing parts when a row was clipped. The column budget above was spent one Unicode scalar at a time, so a character that did fit still lost anything the terminal draws in the same cell: `日` followed by a combining acute accent rendered as a bare `日`, and `👩` followed by a skin-tone modifier rendered as a bare `👩`. The budget now advances one grapheme cluster at a time and a cluster is drawn whole or not at all, which also fixes the reverse error of under-charging: a keycap such as `#️⃣` is three scalars that print in two columns, and counting it as one column let those rows overflow.
- Fixed a wide `--marker` shifting the interactive layout out of alignment: the gutter in front of each result was always charged as two columns, but a row prints the pointer only when it is selected and the marker only when it is marked. `--multi --marker 界` with nothing marked charged three columns for a two-column gutter, so the selected row's background stopped one cell short of the right edge. Each row is now charged for the pointer, blank, and marker it actually prints.
- Fixed a `--marker` or `--pointer` wider than the terminal wrapping every row it appears on. Charging the gutter for the columns it prints (above) shrank the space left for the result text, but nothing bounded the gutter itself, and shrinking the result width saturates at zero and then stops constraining anything. In a 10-column terminal, `--multi --marker 界界界界界` with a row marked painted the one-column pointer plus the ten-column marker into eleven columns and wrapped the row. The gutter is now clipped to the viewport, the pointer first and the marker into whatever remains.
- Fixed a row ending in zero-width characters being reported as too long and given an ellipsis it had not earned. The column budget stopped the moment it was full, leaving a trailing zero-width character unconsumed and therefore looking like dropped content: a two-column prompt of `ab` followed by U+200B rendered in a two-column terminal as `..` rather than `ab`. Zero-width text at the end of a row no longer counts as truncation.
- Fixed a combining mark still being clipped off its base character when an ANSI colour sequence sat between them. Budgeting by grapheme cluster fixed this for plain text, but a control byte ends a cluster, so under `--ansi` a coloured `日` followed by a reset and then a combining acute was two clusters and the accent was dropped at the viewport edge. The scan now runs past a full budget through everything that costs no columns — colour sequences and zero-width scalars alike — and stops at the first character that would actually overflow, so the accent travels with its base and a trailing reset sequence is kept rather than left to leak into later output.
- Fixed `--ignore-case` being defeated by `--literal` and by disabled normalization: the matcher now folds case itself instead of relying on the normalized search key, in fuzzy and in `--exact` mode.
- Fixed case-insensitive matching treating `İ` (U+0130) as a plain `i`, which made `--exact --filter ia` report `İa` as a match even though it contains no `i`: the character's lowercase form is `i` followed by a combining dot above, and only the `i` was kept. Pairwise folding is now applied only where one character maps to exactly one character, since a comparison that expanded one side would report positions into text that no longer lines up with the text as written.
- Fixed the above turning into the opposite error under `--literal`, where `--ignore-case` stopped folding `İ` at all. Refusing to fold it pairwise had left it reachable only through the normalized key, which `--literal` does not build, so `--filter $'i\u0307' --ignore-case --literal` found nothing in `İstanbul.txt` and `--exact` likewise. A case-insensitive comparison that finds nothing is now retried once with the character's full lowercase form written out on both sides, in fuzzy and in `--exact` mode. `--exact --filter ia` still does not match `İa`: writing the mapping out keeps the combining dot that sits between the `i` and the `a`. `İ` is the only character in Unicode with a lowercase form longer than one character, so nothing else is affected; a query that is a prefix of the written-out form (`--exact --filter i` against `İ`) matches, as it already did in every other mode and in v0.1.11. `--algo v2` / `--algo nucleo` under `--literal` is unchanged and still does not fold this character, because it folds with nucleo's own table.
- Fixed a repetitive query making case-insensitive substring search quadratic in the length of each candidate, which let a few long lines stall the search. A query of 20,000 `a`s against a 40,000-character candidate went from 0.19s to 0.006s for ASCII and from 3.5s to 0.009s for non-ASCII text; matching results are unchanged.
- Fixed `--algo fzf-v2` / `--algo nucleo` losing case-insensitive matches for
  characters `nucleo-matcher`'s own case-folding table does not know: `--filter ɤ`
  found nothing in `Ɤx`. 55 characters are affected (`Ɤ` U+A7CB, `Ᲊ` U+1C89, the
  Garay block). Skipping the case-folded search key is now conditional on the matcher
  folding case the same way the index does, which nucleo does not, so those keys stay
  active on that path. Scoring that key back costs the nucleo path 1.8x its search
  time on the 100k plain benchmark (5.6ms to 10.3ms), which is still 0.90x the 0.1.11
  release; end to end over 500k lines, where reading input and building the index
  dominate, `--algo v2` measures 1.00x-1.01x. The default `--algo greedy` is
  unaffected, because yuru's own matchers do fold case the way the index does.
- Fixed `--algo fzf-v2` / `--algo nucleo` ignoring the case policy entirely. Those
  algorithms built their matcher with case-insensitivity hard-coded, so
  `--no-ignore-case` and an uppercase query under default smart case both still
  matched case-insensitively: `printf 'ABC\nabc\n' | yuru --filter abc
  --no-ignore-case --algo nucleo` returned both lines instead of only `abc`. The
  matcher is now configured from the requested policy, on the plain, extended, and
  parallel paths alike, so those searches now behave like the default `--algo
  greedy`. *This* entry changes case-sensitive searches only; it leaves a
  case-insensitive one exactly as it scored before. Case-insensitive output on these
  algorithms is nevertheless **not** byte-identical to 0.1.11, for two reasons
  recorded separately: 0.1.11 aborts outright on many uppercase queries, including
  under an explicit `--ignore-case` (see the crash entry below), and
  `--ignore-case --literal` gains the matches described in the `--literal` entry
  above. Across the case-flag queries crossed with these two backends, every
  case-insensitive invocation that 0.1.11 completed and that does not pass
  `--literal` is byte-identical.
- Fixed `--algo fzf-v2` / `--algo nucleo` **crashing** on a query containing an
  uppercase letter. `nucleo-matcher` requires the caller to case-fold the query
  before handing it over when the matcher is case-insensitive; yuru never did, so
  nucleo's prefilter and its scoring matrix could disagree and abort the process
  with `should have been caught by prefilter`. `yuru --filter ReadMe --algo nucleo`
  over a list containing `lib/ReadMe1.md` exited 101 with no output. Whether a given
  uppercase query aborts depends on the candidates as well as on the query, so the
  failure is not reliably visible from one input: over the mixed-case corpus used
  for the comparison above, `ReadMe`, `READ`, and `AbcDef` all abort - with or
  without `--ignore-case`, `--no-ignore-case`, or `--literal` - while `ABC` and
  `HTTP` complete. The query is
  now folded with nucleo's own table before matching, for every case mode. Deciding
  whether a query needs folding costs a table lookup per character, so it is
  memoized on the query rather than repeated for every candidate; the remaining cost
  is 1.03x-1.05x of the nucleo search path, or 1.08x of the 0.1.11 release for
  `--filter abc --algo nucleo` over 500k lines (0.197s to 0.212s).
- Fixed the exact-case bonus never reaching extended-query exact terms, which left
  `'term`, `^term`, `term$`, `^term$`, and `'term'` ordering case variants by input
  order. Those terms are compared against case-folded text on both sides, so the
  information the bonus needs was gone before scoring: `printf 'FOO\nfoo\n' | yuru
  --filter "'foo" --ignore-case` returned `FOO` first, and reversing the two input
  lines reversed the answer. `'readme` scored `README.md` and `readme.md` at 9991
  apiece, while the single-term `--exact readme` path correctly put the literal
  `readme.md` first. All exact term forms now award the same 75 points on the same
  terms as `--exact`, so the two paths agree and the winner no longer depends on
  input order. Case-sensitive searches still collect nothing and keep their scores
  exactly. A term that normalization changed beyond case (a full-width or kana
  variant) matches as before and takes no bonus, since it was not typed the way any
  candidate spells it.
- Fixed a library embedder's own `MatcherBackend` silently losing matches. A matcher
  passed to `search_with_stats` is never told the search's case policy, so a matcher
  that is case-sensitive by construction reached differently cased candidates only
  through the case-folded search key - which the new `case_fold_only` skip had
  removed, turning `build_index(["ABC"])` plus query `abc` into no match at all. The
  skip now applies only to matchers that report `MatcherBackend::folds_case`, so an
  unmodified embedder behaves as it did in 0.1.11. The `yuru` command line was never
  affected.
- Added `--live-smart-case` as a **preview feature**, off by default. It re-evaluates smart
  case as you type: an uppercase character switches to case-sensitive matching and
  highlighting, and deleting it switches back, like fzf. Without it, case sensitivity is
  derived once from the initial query and stays fixed for the session, exactly as in 0.1.x.
  `--ignore-case` and `--no-ignore-case` remain hard overrides either way.

  **This flag is preview quality and may be changed or removed in a later release. Do not
  depend on it in scripts.** Letting the case policy change mid-session means a result set
  can be on screen that was computed under the previous policy while the replacement search
  is still running, and that has produced real defects during development. See
  [Preview features](docs/fzf-compat.md#preview-features) for the known issues. It is
  shipped off by default so the default interactive path keeps the 0.1.x behavior.
- Fixed interactive `Enter` returning a line the query does not match when it was pressed
  before the search for what you had just typed finished. Results were only checked
  against the query text they were computed for, and never against the case policy, so
  on a 400,000-line list typing `ab` and then `C` and `Enter` in one burst accepted
  `ABC-match` even though the live query `abC` is case-sensitive under smart case and
  excludes it; a lowercase keystroke could likewise accept a row from the previous query.
  Every result set now carries the query *and* the resolved case policy it answers, and
  an `Enter` that arrives while the rows on screen belong to an older search is held
  until the search for the live query lands, then applied to those results (accepting
  nothing if the live query matches nothing). Rows stay on screen and keep repainting
  while that search runs, and `Ctrl-C` / `Esc` still abort immediately.
- Fixed interactive `Enter` accepting a different row than the one under the cursor when
  the result list was replaced between the keystroke and the accept. The selection was a
  row *number* into a list that is swapped out wholesale every time a search lands, so
  the number survived the swap while its meaning did not. Moving with `Ctrl-P` before
  the search for what you had just typed finished, then pressing `Enter`, reapplied the
  old row number to the new rows: on a 100,000-line list, typing `ab`, then `C`,
  `Ctrl-P` and `Enter` in one burst returned the row *below* the one that was
  highlighted. The selection now remembers which candidate it is on and finds that same
  candidate again wherever the replacement puts it. If that candidate is not in the
  replacement at all, the cursor resets to the top like fzf, and an `Enter` that was
  already aimed at the vanished row returns nothing rather than whatever took its place.
  Pressing `Enter` without having moved still takes the top row of the live results.
  This affects the default configuration as well as `--live-smart-case`: any query
  change replaces the list.
- Fixed marked rows being silently dropped when the query was refined before `Enter`.
  With `--multi`, the accepted set was the marks intersected with the *current* result
  list, so marking two files, typing a few more characters, and pressing `Enter`
  returned only the marks that still matched. Marks are identities, not positions:
  everything you marked is now returned, as fzf does. They are also returned in the
  order you marked them rather than in result order; for marks made within a single
  result list from top to bottom, which is the common case, that is the same order as
  before.
- Fixed a superseded Japanese, Korean, or Chinese row being painted as a match. When a
  phonetic key matches something the surface text does not spell (`hangeul` for `한글`),
  the whole row is highlighted, since there is no character to point at. That was done
  without checking that the key still matches what is typed, so a row left on screen
  while its replacement search ran was drawn as a live match: query `h`, then `G`, and
  `한글` stayed fully highlighted although `hG` matches nothing. The fallback now
  applies only while the key the row was scored on still matches the query.
- Fixed the interactive interface not repainting on a terminal resize, which left a stale, wrongly sized frame until the next keystroke.

### Security

- Updated the locked `rkyv` to 0.8.17, which fixes RUSTSEC-2026-0235 (an out-of-bounds
  read reachable through checked deserialization of a crafted archive). `rkyv` reaches
  yuru only as a transitive dependency of the bundled `lindera` Japanese dictionary, so
  no yuru code path deserializes untrusted archives; the advisory nevertheless failed the
  project's own `cargo deny check advisories` gate. Lockfile-only change, no behavior
  difference.

## 0.1.11

- Added readline-style TUI key bindings and Unicode-aware query cursor positioning ([#3](https://github.com/Ameyanagi/yuru/pull/3), contributed by [@gw31415](https://github.com/gw31415)).
- Hardened terminal rendering, preview processes, file and image decoding, SVG loading, worker cleanup, highlighting, and CJK key generation against untrusted or oversized input ([#4](https://github.com/Ameyanagi/yuru/pull/4)).

## 0.1.10

- Fixed PowerShell launches that opened briefly and exited by forcing interactive mode from generated key handlers.
- Ignored Windows key-release events in the TUI so PSReadLine hotkey releases cannot be treated as selection input.
- Drained pending PowerShell console input before starting Yuru from Ctrl-T, Ctrl-R, Alt-C, and completion handlers.

## 0.1.9

- Fixed Windows release installs on older PowerShell/.NET environments by hardening architecture detection.
- Fixed PowerShell profile loading so generated integration scripts are joined before `Invoke-Expression`.
- Fixed PowerShell key bindings and completion so Yuru keeps console input instead of disappearing after launch.
- Added Windows diagnostics and CI smoke checks for PowerShell integration freshness.

## 0.1.8

- Refactored core, CLI, and TUI internals into smaller modules while preserving the existing command-line behavior.
- Simplified backend search APIs with explicit key/query budgets and shared query preparation.
- Improved maintainability of TUI preview, render, action, state, and search-worker code with focused tests.
- Fixed no-input TUI preview/result geometry for reverse layouts.
- Reduced built-in preview text sniffing I/O by reading only the ASCII detection sample window.

## 0.1.7

- Added package README metadata so crates.io renders the README for all published Yuru crates.
- Added rustdoc coverage for the public core, language backend, and TUI APIs.

## 0.1.6

- Fixed the Unix installer so binary-only installs with `--no-shell` / `--no-config`
  exit successfully after installing the release asset.

## 0.1.5

- Added Korean Hangul matching support, including romanization, choseong initials, and Korean 2-set keyboard-layout keys.
- Added built-in preview configuration with `bat` / `cat` text fallback, ASCII text sniffing, internal image rendering, and selectable image protocols.
- Added Chinese `zh.polyphone = "common"` heteronym expansion with capped alternate pinyin keys; reserved `phrase` and `script` options now warn or stay hidden.
- Added README/demo and internals documentation covering multilingual indexing/search complexity, fzf comparison, preview internals, and agentic coding disclosure.
- Hardened release publishing by including `yuru-ko` in the crates.io publish order and validating omitted workspace dependencies before tag releases.
- Clarified matcher algorithm names so `fzf-v1` / `fzf-v2` are documented as compatibility-inspired modes, not byte-for-byte fzf implementations.

## 0.1.4

- Added streaming interactive input for stdin and default source commands, with `--sync` for fzf-style synchronous startup.
- Updated shell integrations to stream `fd` / `fdfind` / `find` output into Yuru instead of preloading command output into temp files.
- Made `CTRL-T`, `ALT-C`, and `**<TAB>` avoid following symlinks by default and skip macOS `Library` in generated shell candidates.
- Suppressed fzf-only UI option warnings such as `--preview` inside shell key bindings.
- Hardened the built-in walker to skip filesystem loop errors when following symlinks.
- Added fzf-style bottom prompt layout support with bottom-up result painting for `--layout=default`, plus `--layout=reverse-list` and `--reverse`.
- Added text preview support for `--preview`, including stderr/failure text, and partial `--color` support for `pointer`, `hl`, and `hl+`.
- Fixed zsh shell integration by avoiding the read-only `status` parameter.
- Improved Japanese numeric date matching so `2025年8月` can match `20258gatsu`, `2025nen8gatsu`, `8gatsu`, `はち`, and literal `2025`.

## 0.1.3

- Added `yuru doctor` for local setup diagnostics.
- Added `--explain` and `--debug-match` for inspecting winning match keys and source spans.
- Added fzf compatibility, configuration, language matching, troubleshooting, install, security, and contributor docs.
- Added README badges, localized README updates, demo assets, and release-pinned install examples.
- Added golden ranking tests, matcher property tests, MSRV checks, and supply-chain audit policy.
- Updated benchmark reporting, including 1M and kanji-heavy benchmark numbers.
- Improved release notes to point at tag-pinned installer scripts.

## 0.1.2

- Renamed the project from Yomi to Yuru.
- Published release assets for Linux, macOS Intel, macOS Apple Silicon, and Windows.
- Published crates.io packages and added release installer smoke tests.
- Improved shell integration for `CTRL-T`, `CTRL-R`, `ALT-C`, and `**<TAB>`.
- Added Lindera-backed Japanese readings, Chinese pinyin source maps, and CJK highlight fixes.
- Added fzf compatibility controls, config precedence, and release-only publishing workflow.

## 0.1.1

- First Yuru release after the rename.
- Added user-space installers and shell configuration support.
- Added localized README files for Japanese, Chinese, and Korean.

## 0.1.0

- Historical Yomi release.
- Implemented the initial fuzzy finder, phonetic matching backends, TUI, shell integration, and benchmarks.
