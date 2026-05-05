# Language Matching

Yuru always keeps direct fuzzy matching enabled. Japanese and Chinese modes add extra candidate keys, then search chooses the best key.

## Japanese

`--lang ja` adds kana and romaji keys. Kanji readings come from Lindera when `--ja-reading lindera` is enabled. Use `--ja-reading none` to disable generated kanji readings.

Examples:

```sh
printf "カメラ.txt\n" | yuru --lang ja --filter kamera
printf "tests/日本語.txt\n" | yuru --lang ja --filter ni
```

Generated reading keys carry source spans. A romaji match can highlight the original Japanese surface text instead of the whole CJK run.

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

`--no-zh-pinyin` disables pinyin keys. `--no-zh-initials` disables initials. `--zh-polyphone` and `--zh-script` are configuration surfaces for evolving phrase/script behavior; current behavior is intentionally conservative and mostly follows the pinyin crate plus a small phrase override set.

## Auto Mode

`--lang auto` chooses one backend before indexing. Locale and query/candidate characters influence the choice. It does not build Japanese and Chinese keys at the same time.

Use `--explain` to inspect the winning key:

```sh
printf "北京大学.txt\n" | yuru --lang zh --filter bjdx --explain
```
