# Language Matching

Yuru always keeps direct fuzzy matching enabled. Japanese, Korean, and Chinese modes add extra candidate keys, then search chooses the best key.

## Japanese

`--lang ja` adds kana and romaji keys. Kanji readings come from Lindera when `--ja-reading lindera` is enabled. Use `--ja-reading none` to disable generated kanji readings.

Width-compatible forms are normalized before matching: full-width ASCII, half-width katakana, full-width spaces, dash variants, and Japanese prolonged sound marks are comparable. For example, ASCII `-` can match `ー` in `ハッピー` or `コード`.

Romaji query expansion accepts common IME-style aliases in addition to canonical reading keys. For example, `zyu` can match `じゅ`, `nn`/`xn` can match `ん`, and small-kana inputs such as `ltsu`/`xtsu` and `lyu`/`xyu` can match `っ` and `ゅ`.

Native kana queries also search generated kana reading keys. Numeric Japanese context keeps the original text for output while adding reading-oriented tokenizer inputs for Arabic numerals, so `8月` can match `8`, `はち`, `hachi`, `8gatsu`, and `gatu`/`gatsu`. For dates, both fully read forms such as `2025nen8gatsu` and compact mixed forms such as `20258gatsu` can match `2025年8月`; standalone `月` remains `tsuki`.

Examples:

```sh
printf "カメラ.txt\n" | yuru --lang ja --filter kamera
printf "tests/日本語.txt\n" | yuru --lang ja --filter ni
printf "重要事項\n" | yuru --lang ja --filter zyu
printf "2025年8月.pdf\n" | yuru --lang ja --filter gatu
```

Generated reading keys carry source spans. A romaji match can highlight the original Japanese surface text instead of the whole CJK run.

## Korean

`--lang ko` adds Hangul-derived keys:

- deterministic romanization with spaces
- joined deterministic romanization
- choseong initials
- Korean 2-set keyboard-layout input

Examples:

```sh
printf "한글.txt\n" | yuru --lang ko --filter hangeul
printf "한글.txt\n" | yuru --lang ko --filter ㅎㄱ
printf "한글.txt\n" | yuru --lang ko --filter gksrmf
```

Korean v1 uses syllable decomposition and deterministic Revised-Romanization-style spelling. It is optimized for fuzzy finder recall and source-span highlighting, not full pronunciation assimilation. For example, `한글` generates `han geul` and `hangeul`; pronunciation-dependent forms such as `같이 -> gachi` or `신라 -> silla` are future work.

`--no-ko-romanization` disables romanized keys. `--no-ko-initials` disables choseong initials. `--no-ko-keyboard` disables Korean 2-set keyboard-layout keys.

## Chinese

`--lang zh` adds pinyin keys:

- full pinyin with spaces
- joined pinyin
- initials

Examples:

```sh
printf "北京大学.txt\n" | yuru --lang zh --filter bjdx
printf "北京大学.txt\n" | yuru --lang zh --filter beijing
```

`--no-zh-pinyin` disables pinyin keys. `--no-zh-initials` disables initials.
`--zh-polyphone none` keeps only the primary reading for each character.
`--zh-polyphone common` also adds a small, capped set of heteronym alternatives,
such as allowing `huanmei` to match `还没`. `phrase` is still accepted for
compatibility, but currently warns and uses `common` behavior. `--zh-script` is
hidden and reserved; non-`auto` values warn because script conversion is not
implemented yet.

## Auto Mode

`--lang auto` chooses one backend before indexing. Locale, query characters, and
the currently available candidate sample influence the choice. It does not build
Japanese, Korean, and Chinese keys at the same time.

## All Mode

`--lang all` builds Japanese, Korean, and Chinese phonetic keys together. Use it
for mixed-language candidate lists where one run should match queries such as
Japanese romaji, Korean romanization, Korean initials, Chinese pinyin, and
Chinese initials.

```sh
printf "北京大学.txt\nカメラ.txt\n한글.txt\n" | yuru --lang all --filter bjdx
printf "北京大学.txt\nカメラ.txt\n한글.txt\n" | yuru --lang all --filter kamera
printf "北京大学.txt\nカメラ.txt\n한글.txt\n" | yuru --lang all --filter hangeul
```

Use `--explain` to inspect the winning key:

```sh
printf "北京大学.txt\n" | yuru --lang zh --filter bjdx --explain
```
