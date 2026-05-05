# Yuru Project Brief

## 1. Project Name

**Yuru**

> Yuru is a fast phonetic fuzzy finder.

日本語説明:

> Yuru は、ローマ字やピンインなどの「読み」で検索できる高速 fuzzy finder です。

想定CLI:

```bash
yuru --lang plain
yuru --lang ja
yuru --lang zh
```

将来的な crate / module 分割案:

```text
yuru          # CLI
yuru-core     # matcher core
yuru-ja       # Japanese backend
yuru-zh       # Chinese backend
yuru-tui      # terminal UI
yuru-cache    # persistent cache, optional
```

---

## 2. Core Idea

Yuru は fzf 風の高速 fuzzy finder に、言語ごとの phonetic search layer を追加する。

主なユースケース:

```text
Japanese:
  query: tokyo
  target: 東京駅
  via: とうきょうえき / toukyoueki / tokyoeki

Chinese:
  query: bjdx
  target: 北京大学
  via: bei jing da xue / beijingdaxue / bjdx
```

重要な設計思想:

```text
検索ループでは読み解析しない。
読み生成は事前生成・遅延生成・永続キャッシュする。
検索時は、既にある search key に対して fuzzy match するだけにする。
```

避けるべき設計:

```text
ユーザーが1文字入力する
  -> 全候補に日本語/中国語の読み解析をかける
  -> romaji/pinyin化する
  -> fuzzy matchする
```

採用する設計:

```text
候補ロード時またはバックグラウンド:
  display text から search keys を生成する
  結果をキャッシュする

キー入力時:
  query variants を少数だけ作る
  既存の search keys に fuzzy match する
```

---

## 3. Non-goals / Important Constraints

### 3.1 複数言語を同時に検索しない

Yuru は、1セッションで1つの language mode だけを有効にする。

```text
--lang=plain
--lang=ja
--lang=zh
--lang=ko   # future
```

つまり、1つの query を日本語・中国語・韓国語として同時に展開しない。

悪い例:

```text
query: han
  Japanese variants
  Chinese pinyin variants
  Korean variants
  plain variants
を全部同時に作る
```

良い例:

```text
--lang=ja:
  han -> はん

--lang=zh:
  han -> pinyin han

--lang=plain:
  han のまま
```

これにより、計算量・メモリ・誤爆率を大きく抑えられる。

### 3.2 ローマ字から漢字への完全変換はしない

ローマ字からかなへの逆マッピングは可能。

```text
tokyo    -> ときょ / とうきょう
shinjuku -> しんじゅく
kyoto    -> きょうと
```

しかし、ローマ字から漢字への一意変換は不可能に近い。

```text
hashi -> 橋 / 箸 / 端
koushi -> 講師 / 孔子 / 公私 / 格子 / 子牛
```

したがって、検索時に `romaji -> kanji` はやらない。

代わりに:

```text
候補側:
  東京駅
  とうきょうえき
  toukyoueki
  tokyoeki

query側:
  tokyo
  ときょ
  とうきょう
```

の両方を使って fuzzy match する。

### 3.3 Original fuzzy search は常に残す

`--lang=ja` や `--lang=zh` でも、元文字列の fuzzy search は常に有効にする。

理由:

```text
README.md
src/main.rs
tokyo_notes.md
東京駅.txt
```

のように、非CJK候補も同じセッションで検索するため。

---

## 4. Language Modes

### 4.1 plain mode

fzf相当の基本モード。

```text
- original string
- normalized string
- lowercase
- NFKC/NFC normalization if needed
```

読み生成なし。最速。

### 4.2 Japanese mode

目的:

```text
東京駅       <- tokyo / toukyoueki / tokyoeki
新宿         <- shinjuku
カメラ       <- kamera / camera
この素晴らしい世界 <- konosuba-style query, future/reference
```

候補側 keys:

```text
Original
Normalized
KanaReading        # とうきょうえき
RomajiReading      # toukyoueki / tokyoeki
LearnedAlias       # user selectionから学習
```

query variants:

```text
original:
  tokyo

romaji -> hiragana:
  ときょ

romaji -> hiragana with long-vowel guesses:
  とうきょう
  とおきょお

hiragana -> katakana:
  トキョ
  トウキョウ
```

注意点:

```text
- 漢字 -> 正しい読み は重い・難しい
- 形態素解析は検索ループ外で行う
- 初期MVPでは、かな/カタカナ候補の変換と alias/cache を優先
- 漢字読み推定は lazy backend として後から追加できるようにする
```

### 4.3 Chinese mode

目的:

```text
北京大学 <- beijingdaxue / bei jing da xue / bjdx
重庆     <- chongqing
```

候補側 keys:

```text
Original
Normalized
PinyinFull       # bei jing da xue
PinyinJoined     # beijingdaxue
PinyinInitials   # bjdx
LearnedAlias
```

注意点:

```text
- 多音字がある
- 全組み合わせを無制限に出さない
- 1候補あたり search key 数に上限を設ける
```

### 4.4 Korean mode, future

将来対応候補。

```text
- Hangul jamo
- 初声検索
```

ただし MVP では不要。

---

## 5. Data Structures

基本構造:

```rust
pub struct Candidate {
    pub display: String,
    pub keys: Vec<SearchKey>,
}

pub struct SearchKey {
    pub text: String,
    pub kind: KeyKind,
    pub weight: i32,
}

pub enum KeyKind {
    Original,
    Normalized,

    // Japanese
    KanaReading,
    RomajiReading,

    // Chinese
    PinyinFull,
    PinyinJoined,
    PinyinInitials,

    // Future
    HangulJamo,
    HangulInitials,

    LearnedAlias,
}
```

Query variant:

```rust
pub struct QueryVariant {
    pub text: String,
    pub kind: QueryVariantKind,
    pub weight: i32,
}

pub enum QueryVariantKind {
    Original,
    Normalized,
    RomajiToKana,
    Pinyin,
    Initials,
}
```

Language backend trait:

```rust
pub enum LangMode {
    Plain,
    Japanese,
    Chinese,
    Korean,
}

pub trait LanguageBackend {
    fn mode(&self) -> LangMode;

    fn normalize_candidate(&self, text: &str) -> String;

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey>;

    fn expand_query(&self, query: &str) -> Vec<QueryVariant>;
}
```

重要:

```text
--lang=ja のときは Pinyin keys を作らない。
--lang=zh のときは Japanese reading keys を作らない。
Original / Normalized は常に作る。
```

---

## 6. Search Pipeline

推奨フロー:

```text
1. 候補をロードする
2. Original / Normalized keys を作る
3. language-specific keys を作る
   - ただし重い読み生成は lazy + cache
4. query を受け取る
5. query variants を最大 V 個まで作る
6. key kind を絞る
7. Stage 0: exact / contains / prefix prefilter
8. Stage 1: fast fuzzy score
9. Stage 2: 上位 B 件だけ high-quality score
10. 表示
11. 選択結果から learned alias を更新する
```

key kind filtering の例:

```text
query variant: Original
  -> Original / Normalized / RomajiReading / PinyinJoined など

query variant: RomajiToKana
  -> KanaReading のみ

query variant: Initials
  -> PinyinInitials / learned alias のみ
```

全 variant と全 key の総当たりは避ける。

---

## 7. Matching Strategy

fzf/fzy/nucleo系の subsequence fuzzy matching を基本にする。

### Stage 0: prefilter

安いフィルタ:

```text
- substring contains
- prefix match
- token contains
- first-character check
- ASCII fast path
```

Rust候補:

```text
memchr / memmem
aho-corasick, only if multiple patterns are useful
```

### Stage 1: fast fuzzy matcher

候補全体にかける高速 matcher。

```text
- subsequence match
- greedy scoring
- O(n) per candidateに近いもの
```

### Stage 2: high-quality scoring for top B

全候補に重い DP をかけない。

```text
Stage 1 の上位 B 件だけに詳細スコアをかける。
B は 1000〜5000 程度からベンチで決める。
```

---

## 8. Complexity Notes

記号:

```text
N = candidate count
L = average candidate length
q = query length
T = worker thread count
V = query variants count
S = total search-key length per candidate
C_lang = language-specific keys を持つ候補数
L_lang = language-specific key length
R(L) = reading generation cost
```

fzf-like baseline:

```text
fast greedy:
  O(NL / T)

high-quality DP-ish fuzzy:
  O(NLq / T)
```

読み対応を雑に総当たりすると:

```text
O(N * V * S * q / T)
```

fzf比:

```text
V * S / L 倍
```

key kind を分離した設計:

```text
O((N * L * q + C_lang * V_lang * L_lang * q) / T)
```

fzf比の目安:

```text
1 + (C_lang / N) * V_lang * (L_lang / L)
```

検索時に読み解析すると:

```text
O(N * R(L) + N * V * S * q / T)
```

これは避ける。

キャッシュありの build cost:

```text
O(NL + (1 - cache_hit_rate) * P * R(L))
```

`P` は読み生成が必要な候補数。

設計目標:

```text
K = search keys per candidate を固定上限にする
V = query variants を固定上限にする
B = high-quality scoring対象数を固定上限にする
```

これにより、検索時の漸近オーダーは fzf に近く保てる。

---

## 9. Caps / Limits

誤爆・遅延・メモリ爆発を防ぐため、上限を必ず入れる。

推奨初期値:

```text
max_query_variants = 16
max_search_keys_per_candidate = 16
max_total_key_bytes_per_candidate = 512 or 1024
max_pinyin_variants_per_candidate = 8
max_romaji_variants_per_candidate = 8
top_b_for_quality_score = 1000 or 5000
```

MVPではもっと小さくしてよい:

```text
max_query_variants = 8
max_search_keys_per_candidate = 8
top_b_for_quality_score = 1000
```

---

## 10. Cache Design

読み生成は永続キャッシュする。

キャッシュキー例:

```rust
pub struct CacheKey {
    pub text_hash: u64,
    pub lang: LangMode,
    pub generator_version: u32,
    pub dictionary_version: u32,
}
```

ファイルパス候補向け:

```rust
pub struct FileCacheKey {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub size: u64,
    pub lang: LangMode,
    pub generator_version: u32,
}
```

キャッシュ内容:

```text
display: 東京駅.txt
keys:
  - 東京駅.txt
  - とうきょうえき.txt
  - toukyoueki.txt
  - tokyoeki.txt
```

lang別に分ける:

```text
~/.cache/yuru/plain/
~/.cache/yuru/ja/
~/.cache/yuru/zh/
```

または1つのDBで `lang` を key に含める。

候補DB:

```text
SQLite
redb
sled
RocksDB
msgpack/jsonl file
```

MVPは単純なJSONLやbincodeでもよい。

---

## 11. Learned Alias

読み推定が難しい固有名詞・地名・人名は、ユーザー選択から学習する。

例:

```text
query: nihonbashi
selected: 日本橋

=> alias追加:
  日本橋 -> nihonbashi
```

別の読みも追加できる:

```text
query: nipponbashi
selected: 日本橋

=> aliases:
  nihonbashi
  nipponbashi
```

LearnedAlias はスコアを高めにする。

スコア方針例:

```text
Original exact/prefix     high
Original fuzzy            high
Learned alias             high-mid
Language reading          mid
Query expansion derived   lower-mid
```

---

## 12. Japanese Romaji -> Kana Reverse Mapping

`romaji -> kana` は trie / longest match で実装できる。

基本テーブル例:

```text
a   -> あ
i   -> い
u   -> う
e   -> え
o   -> お

ka  -> か
ki  -> き
ku  -> く
ke  -> け
ko  -> こ

shi -> し
si  -> し
chi -> ち
ti  -> ち
tsu -> つ
tu  -> つ

kya -> きゃ
kyu -> きゅ
kyo -> きょ

sha -> しゃ
shu -> しゅ
sho -> しょ

ja  -> じゃ
ju  -> じゅ
jo  -> じょ
jya -> じゃ
jyu -> じゅ
jyo -> じょ
```

`n` の扱い:

```text
n + consonant -> ん
n'            -> ん
nn            -> ん + next
n + y         -> にゃ/にゅ/にょ or んや/んゆ/んよ ambiguity
```

曖昧な場合は複数候補を出す。

```text
kanya -> かにゃ / かんや
```

長音・表記揺れ:

```text
tokyo -> ときょ / とうきょう
kyoto -> きょと / きょうと
osaka -> おさか / おおさか
kobe  -> こべ / こうべ
```

ただし特殊処理を入れすぎない。alias学習で補う。

---

## 13. Chinese Pinyin Strategy

候補側 keys:

```text
北京大学
bei jing da xue
beijingdaxue
bjdx
```

多音字があるので、候補を無制限に出さない。

```text
max_pinyin_variants_per_candidate = 8
```

full pinyin / joined pinyin / initials を最低限持つ。

---

## 14. External Libraries to Evaluate

### Rust candidates

```text
nucleo
  - fzf-like fuzzy matcherの本命候補
  - large interactive finder向け
  - worker/snapshot型設計が参考になる

nucleo-matcher
  - 低レベルmatcher API

skim
  - Rust製fzf-like fuzzy finder
  - 比較対象または参考実装

ib-matcher
  - Japanese romaji matching / Chinese pinyin matching に近い
  - そのまま中核にするより、言語処理の参考・比較対象として評価

ib-pinyin
  - Chinese pinyin search用の参考候補

pinyin
  - 中国語pinyin生成候補

memchr
  - substring/prefilter用

aho-corasick
  - 複数pattern同時検索用

regex / regex-automata
  - regex mode用

unicode-normalization
  - NFKC/NFCなど

unicode-segmentation
  - grapheme対応が必要なら

redb / sled / sqlite
  - cache用

tantivy
  - 将来的な巨大index mode用。MVPでは不要
```

### Non-Rust references

```text
fzf
  - v1 greedy, v2 DP-ish high-quality scoringの参考

fzy
  - small/simple fuzzy scoringの参考

RapidFuzz
  - typo tolerant / edit distance系の参考。ただしfzf系とは別物

Fuse.js
  - Bitap typo tolerant searchの参考。fzf系とは別物

Lucene / Tantivy
  - 永続index + fuzzy term searchの参考。CLI stdin型fzfとは別物
```

---

## 15. ib-matcher Positioning

`ib-matcher` は、Yuru の問題設定に近い Rust ライブラリ。

特に近い点:

```text
- Japanese romaji matching
- Chinese pinyin matching
- Unicode-aware matching
- string/glob/regex matching
```

ただし、Yuru の中核にそのまま使えるかはベンチが必要。

懸念点:

```text
- fzf風ランキング/スコアリングとは役割が違う可能性がある
- 大量候補での interactive latency を検証する必要がある
- top-k ranking, worker, incremental update は別設計が必要かもしれない
```

推奨:

```text
C案:
  matcher/ranking は nucleo or 自作 fzf-like core
  Japanese/Chinese phonetic処理は ib-matcher/ib-pinyin を参考・比較
```

---

## 16. CLI Design

基本:

```bash
yuru --lang plain
yuru --lang ja
yuru --lang zh
```

読み生成モード:

```bash
yuru --reading none       # 読み解析なし。最速
yuru --reading lazy       # default. 軽いkeyを先に作り、重い読みは遅延生成
yuru --reading sync       # 起動時に全解析。小さいリスト向け
yuru --reading cache-only # キャッシュにある読みだけ使う
```

その他:

```bash
yuru --no-cache
yuru --cache-dir ~/.cache/yuru
yuru --algo fast
yuru --algo quality
yuru --top-b 1000
yuru --max-query-variants 8
yuru --max-keys-per-candidate 8
```

fzf互換を意識するなら将来的に:

```bash
yuru --exact
yuru --regex
yuru --query <QUERY>
yuru --select-1
yuru --exit-0
```

---

## 17. MVP Plan

### v0.1: plain fuzzy finder

```text
- stdinから候補を読む
- original/normalized keyを作る
- simple fuzzy matcherを実装 or nucleoを使う
- top resultsを表示
- --lang plain を実装
```

### v0.2: Japanese light mode

```text
- --lang ja
- romaji -> kana query expansion
- kana/katakana normalization
- かな/カタカナ候補の romaji key generation
- 漢字読み推定はまだやらない or cache/aliasのみ
```

### v0.3: cache + alias

```text
- persistent cache
- selected resultから learned alias を保存
- query -> selected display のaliasを次回検索に反映
```

### v0.4: Chinese mode

```text
- --lang zh
- pinyin full/joined/initials key generation
- 多音字variantに上限
```

### v0.5: lazy reading generation

```text
- 重い読み生成をworkerで遅延実行
- UIは先に表示
- 読みkeyが増えたらincrementalに更新
```

### v1.0

```text
- plain/ja/zhが実用速度で動く
- cache/aliasが安定
- benchmarkあり
- fzf-like optionsの一部互換
```

---

## 18. Benchmark Plan

データセット:

```text
10k candidates
100k candidates
1M candidates
```

候補内訳:

```text
ASCII paths
Japanese filenames
Chinese filenames
Mixed paths
Long paths
Short names
```

測るもの:

```text
- startup time
- first result latency
- per-keypress latency p50/p95/p99
- memory usage
- cache hit rate
- reading generation time
- top-10 result quality
- false positive rate
```

query examples:

```text
plain:
  src
  main
  readme
  config

Japanese:
  tokyo
  shinjuku
  kyoto
  nihonbashi
  kamera

Chinese:
  bjdx
  beijing
  chongqing
  shanghai
```

比較対象:

```text
fzf
skim
nucleo example or custom harness
ib-matcher direct matching
Yuru with/without prefilter
Yuru with/without cache
```

---

## 19. Key Engineering Decisions

```text
1. Project name is Yuru.
2. Yuru is a phonetic fuzzy finder.
3. One language mode per session.
4. Original fuzzy matching is always enabled.
5. Reading generation never runs in the keypress hot path.
6. Romaji -> kana is allowed and returns multiple capped candidates.
7. Romaji -> kanji is not attempted.
8. Candidate search keys are generated/cached ahead of time or lazily.
9. Query variants and search keys have hard caps.
10. Language-specific backends are separated.
11. Japanese and Chinese are implemented independently.
12. Learned alias is important for difficult names.
13. Use benchmarking to choose nucleo vs self matcher vs ib-matcher integration.
```

---

## 20. Minimal Implementation Sketch

```rust
fn main() -> anyhow::Result<()> {
    let config = Config::from_args();
    let backend = create_backend(config.lang);

    let candidates = read_candidates_from_stdin()?;

    let indexed: Vec<Candidate> = candidates
        .into_iter()
        .map(|display| {
            let mut keys = Vec::new();
            keys.push(SearchKey::original(&display));
            keys.push(SearchKey::normalized(backend.normalize_candidate(&display)));
            keys.extend(backend.build_candidate_keys(&display));
            dedup_and_limit_keys(keys, config.max_keys_per_candidate);

            Candidate { display, keys }
        })
        .collect();

    run_ui(indexed, backend, config)?;

    Ok(())
}

fn search(
    query: &str,
    candidates: &[Candidate],
    backend: &dyn LanguageBackend,
    config: &Config,
) -> Vec<ScoredCandidate> {
    let variants = dedup_and_limit_variants(
        backend.expand_query(query),
        config.max_query_variants,
    );

    let mut scored = Vec::new();

    for cand in candidates {
        if let Some(score) = score_candidate(&variants, cand) {
            scored.push(ScoredCandidate {
                display: cand.display.clone(),
                score,
            });
        }
    }

    scored.sort_by(|a, b| b.score.cmp(&a.score));
    scored.truncate(config.limit);
    scored
}
```

---

## 21. Open Questions for Codex / Implementation

```text
- Use nucleo directly, or implement a small fzy/fzf-like matcher first?
- Which cache backend should MVP use?
- Should Japanese kanji reading be plugin-based from the start?
- Which pinyin crate gives the best accuracy/performance?
- How to represent per-key weights with nucleo if using external matcher?
- How much fzf CLI compatibility is necessary for v1?
- How to handle async/lazy key updates without flicker?
```

Recommended first coding task:

```text
Create a Rust CLI prototype named yuru with:
  - --lang plain|ja|zh
  - stdin candidate loading
  - Candidate/SearchKey data model
  - plain fuzzy matching
  - Japanese romaji->kana query expansion
  - hard caps for query variants/search keys
  - simple benchmark harness
```
---

## 22. Codex Addendum: Test Plan

This section is intentionally concrete. Codex should be able to create files, test names, fixtures, and initial assertions from it.

### 22.1 Testing policy

Required testing layers:

```text
Unit tests:
  Pure functions: normalization, romaji/kana conversion, pinyin key generation,
  query expansion, key caps, scoring, sorting, cache keys.

Integration tests:
  CLI behavior via stdin + --query. Do not require a TUI for automated tests.

Property/fuzz tests:
  Unicode inputs, invalid-looking romaji, long strings, empty strings, caps.

Benchmark tests:
  Criterion or Divan benchmarks for 10k/100k/1M synthetic candidates.
```

For testability, implement a non-interactive query mode early:

```bash
yuru --lang ja --query tokyo --limit 10 < fixtures/mixed.txt
```

This is more important than the TUI in the first prototype.

### 22.2 Suggested test crates

```toml
[dev-dependencies]
assert_cmd = "*"          # CLI integration tests
predicates = "*"          # stdout/stderr assertions
insta = "*"               # snapshot/golden ranking tests
proptest = "*"            # property tests
criterion = "*"           # benchmarks
# or: divan = "*"         # simpler benchmark harness alternative
tempfile = "*"            # cache tests
rstest = "*"              # table-driven tests
pretty_assertions = "*"   # readable diffs
```

Do not make perf benchmarks strict pass/fail based on wall-clock time across all machines. Use benchmarks to record baseline and catch large regressions locally/CI with a relaxed threshold if needed.

### 22.3 Unit tests: normalization

Target module names:

```text
crates/yuru-core/src/normalize.rs
crates/yuru-core/src/kana.rs
```

Test cases:

```rust
#[test]
fn normalize_ascii_lowercase() {
    assert_eq!(normalize("README.MD"), "readme.md");
}

#[test]
fn normalize_fullwidth_ascii_nfkc() {
    assert_eq!(normalize("ＡＢＣ１２３"), "abc123");
}

#[test]
fn normalize_halfwidth_katakana() {
    assert_eq!(normalize("ｶﾒﾗ"), "カメラ");
}

#[test]
fn katakana_to_hiragana_basic() {
    assert_eq!(katakana_to_hiragana("カメラ"), "かめら");
}

#[test]
fn hiragana_to_katakana_basic() {
    assert_eq!(hiragana_to_katakana("しんじゅく"), "シンジュク");
}
```

If the first MVP does not implement full NFKC, mark the test as pending or gate it behind a feature, but keep the expected behavior documented.

### 22.4 Unit tests: Japanese romaji -> kana query expansion

Target module:

```text
crates/yuru-ja/src/romaji.rs
```

The important design is that conversion returns multiple capped candidates, not one canonical answer.

Required tests:

```rust
#[test]
fn romaji_shinjuku() {
    let out = romaji_to_kana_candidates("shinjuku", 8);
    assert!(out.contains(&"しんじゅく".to_string()));
}

#[test]
fn romaji_tokyo_has_short_and_long_vowel_candidates() {
    let out = romaji_to_kana_candidates("tokyo", 8);
    assert!(out.contains(&"ときょ".to_string()));
    assert!(out.contains(&"とうきょう".to_string()));
}

#[test]
fn romaji_kyoto_has_long_vowel_candidate() {
    let out = romaji_to_kana_candidates("kyoto", 8);
    assert!(out.contains(&"きょうと".to_string()));
}

#[test]
fn romaji_double_consonant_to_small_tsu() {
    let out = romaji_to_kana_candidates("gakkou", 8);
    assert!(out.contains(&"がっこう".to_string()));
}

#[test]
fn romaji_n_before_consonant() {
    let out = romaji_to_kana_candidates("kanpai", 8);
    assert!(out.contains(&"かんぱい".to_string()));
}

#[test]
fn romaji_n_apostrophe() {
    let out = romaji_to_kana_candidates("shin'ya", 8);
    assert!(out.contains(&"しんや".to_string()));
}

#[test]
fn romaji_n_y_ambiguity_is_capped() {
    let out = romaji_to_kana_candidates("kanya", 8);
    assert!(out.contains(&"かにゃ".to_string()));
    assert!(out.contains(&"かんや".to_string()));
    assert!(out.len() <= 8);
}

#[test]
fn romaji_variants_are_deduped_and_capped() {
    let out = romaji_to_kana_candidates("oooooooooooooooo", 4);
    assert!(out.len() <= 4);
    assert_eq!(out.len(), out.iter().collect::<std::collections::HashSet<_>>().len());
}
```

MVP can use a simple custom trie/longest-match implementation. Later, compare with `wana_kana` or `romkan`, but do not let a third-party library decide Yuru's query variant policy without caps.

### 22.5 Unit tests: Chinese pinyin key generation

Target module:

```text
crates/yuru-zh/src/pinyin.rs
```

Required tests:

```rust
#[test]
fn pinyin_beijing_university_keys() {
    let keys = build_pinyin_keys("北京大学", 8);
    assert!(keys.contains(&"bei jing da xue".to_string()));
    assert!(keys.contains(&"beijingdaxue".to_string()));
    assert!(keys.contains(&"bjdx".to_string()));
}

#[test]
fn pinyin_chongqing_expected_common_reading() {
    let keys = build_pinyin_keys("重庆", 8);
    assert!(keys.iter().any(|k| k.contains("chongqing") || k.contains("chong qing")));
}

#[test]
fn pinyin_variants_are_capped() {
    let keys = build_pinyin_keys("重庆银行重庆分行", 4);
    assert!(keys.len() <= 4);
}

#[test]
fn pinyin_empty_input_is_empty_or_original_only() {
    let keys = build_pinyin_keys("", 8);
    assert!(keys.is_empty());
}
```

If using a pinyin crate that returns several readings per character, never generate the full Cartesian product without a cap or beam.

### 22.6 Unit tests: candidate key generation

Target module:

```text
crates/yuru-core/src/candidate.rs
```

Required tests:

```rust
#[test]
fn plain_mode_only_original_and_normalized() {
    let cand = build_candidate("東京駅", LangMode::Plain, test_config());
    assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
    assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Normalized));
    assert!(!cand.keys.iter().any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading | KeyKind::PinyinFull)));
}

#[test]
fn japanese_mode_does_not_build_pinyin_keys() {
    let cand = build_candidate("東京駅", LangMode::Japanese, test_config());
    assert!(!cand.keys.iter().any(|k| matches!(k.kind, KeyKind::PinyinFull | KeyKind::PinyinJoined | KeyKind::PinyinInitials)));
}

#[test]
fn chinese_mode_does_not_build_japanese_reading_keys() {
    let cand = build_candidate("北京大学", LangMode::Chinese, test_config());
    assert!(!cand.keys.iter().any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading)));
}

#[test]
fn original_key_is_always_present() {
    for lang in [LangMode::Plain, LangMode::Japanese, LangMode::Chinese] {
        let cand = build_candidate("README.md", lang, test_config());
        assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
    }
}

#[test]
fn search_keys_are_deduped_and_capped() {
    let mut cfg = test_config();
    cfg.max_search_keys_per_candidate = 4;
    let cand = build_candidate("カメラ camera CAMERA", LangMode::Japanese, cfg);
    assert!(cand.keys.len() <= 4);
}
```

### 22.7 Unit tests: query expansion

Target module:

```text
crates/yuru-core/src/query.rs
crates/yuru-ja/src/query.rs
crates/yuru-zh/src/query.rs
```

Required tests:

```rust
#[test]
fn plain_query_expansion_is_small() {
    let vars = expand_query("Tokyo", LangMode::Plain, 8);
    assert!(vars.iter().any(|v| v.text == "tokyo"));
    assert!(vars.len() <= 2);
}

#[test]
fn japanese_query_tokyo_expands_to_kana() {
    let vars = expand_query("tokyo", LangMode::Japanese, 8);
    assert!(vars.iter().any(|v| v.text == "tokyo"));
    assert!(vars.iter().any(|v| v.text == "ときょ"));
    assert!(vars.iter().any(|v| v.text == "とうきょう"));
    assert!(vars.len() <= 8);
}

#[test]
fn chinese_query_bjdx_keeps_initials() {
    let vars = expand_query("bjdx", LangMode::Chinese, 8);
    assert!(vars.iter().any(|v| v.text == "bjdx"));
    assert!(vars.len() <= 8);
}

#[test]
fn empty_query_does_not_panic() {
    let vars = expand_query("", LangMode::Japanese, 8);
    assert!(vars.len() <= 1);
}
```

### 22.8 Unit tests: fuzzy matcher and ranking

Target module:

```text
crates/yuru-core/src/matcher.rs
crates/yuru-core/src/rank.rs
```

Required tests:

```rust
#[test]
fn subsequence_match_basic() {
    assert!(fast_fuzzy_score("abc", "a_b_c").is_some());
    assert!(fast_fuzzy_score("abc", "acb").is_none());
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
fn reading_match_scores_below_original_exact() {
    let original = score_key("tokyo", SearchKey::original("tokyo")).unwrap();
    let reading = score_key("tokyo", SearchKey::romaji_reading("tokyoeki")).unwrap();
    assert!(original > reading);
}

#[test]
fn learned_alias_scores_high_enough() {
    let alias = score_key("nihonbashi", SearchKey::learned_alias("nihonbashi")).unwrap();
    let reading = score_key("nihonbashi", SearchKey::romaji_reading("nihonbashieki")).unwrap();
    assert!(alias >= reading);
}

#[test]
fn sorting_is_deterministic_on_equal_scores() {
    let results = search_with_ids("abc", fixture_candidates_with_equal_scores());
    assert!(is_sorted_by_score_then_stable_id(&results));
}
```

Avoid testing exact numeric scores unless the scoring formula is intentionally frozen. Prefer relative ordering tests.

### 22.9 Unit tests: key-kind filtering

Required tests:

```rust
#[test]
fn romaji_to_kana_variant_only_targets_kana_keys() {
    let variant = QueryVariant::romaji_to_kana("とうきょう");
    assert!(key_kind_allowed(&variant, KeyKind::KanaReading));
    assert!(!key_kind_allowed(&variant, KeyKind::PinyinJoined));
}

#[test]
fn pinyin_initial_variant_only_targets_pinyin_initials_and_aliases() {
    let variant = QueryVariant::initials("bjdx");
    assert!(key_kind_allowed(&variant, KeyKind::PinyinInitials));
    assert!(key_kind_allowed(&variant, KeyKind::LearnedAlias));
    assert!(!key_kind_allowed(&variant, KeyKind::KanaReading));
}
```

This protects the core requirement: do not cross-product all languages and all key kinds.

### 22.10 Unit tests: cache and hot path

Target module:

```text
crates/yuru-cache/src/lib.rs
```

Required tests:

```rust
#[test]
fn cache_key_includes_language() {
    let ja = CacheKey::new("東京駅", LangMode::Japanese, 1, 1);
    let zh = CacheKey::new("東京駅", LangMode::Chinese, 1, 1);
    assert_ne!(ja, zh);
}

#[test]
fn cache_key_includes_generator_version() {
    let v1 = CacheKey::new("東京駅", LangMode::Japanese, 1, 1);
    let v2 = CacheKey::new("東京駅", LangMode::Japanese, 2, 1);
    assert_ne!(v1, v2);
}

#[test]
fn cache_roundtrip_search_keys() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open(dir.path()).unwrap();
    let key = CacheKey::new("東京駅", LangMode::Japanese, 1, 1);
    let keys = vec![SearchKey::kana_reading("とうきょうえき"), SearchKey::romaji_reading("tokyoeki")];
    cache.put(&key, &keys).unwrap();
    assert_eq!(cache.get(&key).unwrap(), Some(keys));
}

#[test]
fn search_hot_path_does_not_call_reading_generator() {
    let backend = MockBackend::with_generation_counter();
    let index = build_index_once(vec!["東京駅".into()], &backend);
    backend.reset_counter();
    let _ = search("tokyo", &index, &backend, test_config());
    assert_eq!(backend.generation_count(), 0);
}
```

This is one of the most important tests in the project.

### 22.11 Unit tests: learned alias

Required tests:

```rust
#[test]
fn selected_candidate_adds_alias() {
    let mut aliases = AliasStore::in_memory();
    aliases.record_selection("nihonbashi", "日本橋").unwrap();
    assert!(aliases.aliases_for("日本橋").unwrap().contains(&"nihonbashi".to_string()));
}

#[test]
fn learned_alias_affects_next_search() {
    let mut index = build_index(vec!["日本橋".into(), "日本語".into()], LangMode::Japanese);
    index.record_selection_alias("nihonbashi", "日本橋");
    let results = search("nihonbashi", &index.candidates, &JapaneseBackend::default(), test_config());
    assert_eq!(results[0].display, "日本橋");
}
```

### 22.12 CLI integration tests

Use `assert_cmd` and a non-interactive `--query` mode.

Fixture:

```text
fixtures/mixed_paths.txt
README.md
src/main.rs
docs/tokyo_notes.md
東京駅.txt
新宿メモ.txt
北京大学.txt
重庆.txt
カメラ.txt
```

Required tests:

```rust
#[test]
fn cli_plain_query_readme() {
    Command::cargo_bin("yuru")
        .unwrap()
        .args(["--lang", "plain", "--query", "read", "--limit", "1"])
        .write_stdin(include_str!("fixtures/mixed_paths.txt"))
        .assert()
        .success()
        .stdout(predicate::str::contains("README.md"));
}

#[test]
fn cli_ja_query_kamera_matches_katakana() {
    Command::cargo_bin("yuru")
        .unwrap()
        .args(["--lang", "ja", "--query", "kamera", "--limit", "3"])
        .write_stdin(include_str!("fixtures/mixed_paths.txt"))
        .assert()
        .success()
        .stdout(predicate::str::contains("カメラ.txt"));
}

#[test]
fn cli_ja_query_tokyo_matches_when_reading_key_or_alias_exists() {
    Command::cargo_bin("yuru")
        .unwrap()
        .args(["--lang", "ja", "--query", "tokyo", "--limit", "3", "--alias", "tokyo=東京駅.txt"])
        .write_stdin(include_str!("fixtures/mixed_paths.txt"))
        .assert()
        .success()
        .stdout(predicate::str::contains("東京駅.txt"));
}

#[test]
fn cli_zh_query_bjdx_matches_beijing_university() {
    Command::cargo_bin("yuru")
        .unwrap()
        .args(["--lang", "zh", "--query", "bjdx", "--limit", "3"])
        .write_stdin(include_str!("fixtures/mixed_paths.txt"))
        .assert()
        .success()
        .stdout(predicate::str::contains("北京大学.txt"));
}

#[test]
fn cli_caps_query_variants() {
    Command::cargo_bin("yuru")
        .unwrap()
        .args(["--lang", "ja", "--query", "oooooooo", "--max-query-variants", "4", "--debug-query-variants"])
        .write_stdin(include_str!("fixtures/mixed_paths.txt"))
        .assert()
        .success()
        .stdout(predicate::str::contains("variant_count=4"));
}
```

The `--alias` flag above can be a test-only/debug option. Alternatively, use a temporary alias/cache file.

### 22.13 Snapshot tests for ranking

Use `insta` for top-N snapshots. Keep fixtures small and deterministic.

Example snapshot groups:

```text
plain_src_top10.snap
ja_tokyo_top10.snap
ja_kamera_top10.snap
zh_bjdx_top10.snap
zh_chongqing_top10.snap
```

Snapshot output format should be stable:

```text
score<TAB>key_kind<TAB>display
```

Do not include timing or absolute paths in snapshots.

### 22.14 Property tests

Use `proptest`.

Required properties:

```rust
proptest! {
    #[test]
    fn romaji_conversion_never_panics(s in "[A-Za-z' -]{0,128}") {
        let out = romaji_to_kana_candidates(&s, 16);
        prop_assert!(out.len() <= 16);
    }

    #[test]
    fn query_expansion_is_capped_for_any_unicode_input(s in ".{0,256}") {
        let vars = expand_query(&s, LangMode::Japanese, 8);
        prop_assert!(vars.len() <= 8);
    }

    #[test]
    fn candidate_key_generation_is_capped(s in ".{0,256}") {
        let cfg = Config { max_search_keys_per_candidate: 8, max_total_key_bytes_per_candidate: 1024, ..test_config() };
        let cand = build_candidate(&s, LangMode::Japanese, cfg);
        prop_assert!(cand.keys.len() <= 8);
        prop_assert!(cand.keys.iter().map(|k| k.text.len()).sum::<usize>() <= 1024);
    }

    #[test]
    fn search_never_panics_on_unicode(query in ".{0,64}", item in ".{0,256}") {
        let cand = Candidate::from_original(item);
        let _ = score_candidate(&expand_query(&query, LangMode::Plain, 4), &cand);
    }
}
```

### 22.15 Benchmark tests

Suggested benchmark files:

```text
benches/search_plain.rs
benches/search_ja.rs
benches/search_zh.rs
benches/keygen.rs
benches/query_expand.rs
```

Datasets:

```text
10k candidates
100k candidates
1M candidates, optional local benchmark
```

Bench cases:

```text
Plain:
  src, main, readme, config

Japanese:
  tokyo, shinjuku, kyoto, nihonbashi, kamera

Chinese:
  bjdx, beijing, chongqing, shanghai
```

Metrics to record:

```text
- index build time
- query expansion time
- per-query search latency
- allocations per query, if possible
- total candidate-key bytes
- memory usage, if measured externally
- cache hit/miss build time
```

Bench variants:

```text
- custom fast matcher only
- custom fast + top-B quality score
- nucleo backend
- memchr prefilter on/off
- key-kind filtering on/off
- cache hit vs cache miss
```

Instrumentation counters to add in debug/bench builds:

```rust
pub struct SearchStats {
    pub candidates_seen: usize,
    pub keys_seen: usize,
    pub variants_seen: usize,
    pub fuzzy_calls: usize,
    pub quality_score_calls: usize,
    pub reading_generation_calls: usize,
}
```

Benchmark assertions should check counters, not just time:

```text
- reading_generation_calls == 0 during search
- variants_seen <= max_query_variants
- quality_score_calls <= top_b_for_quality_score * max_query_variants
- keys_seen is much lower with key-kind filtering than without it
```

---

## 23. Codex Addendum: Big-O Estimation

### 23.1 Symbols

```text
N       = number of candidates
L       = average original candidate length
q       = query length
T       = worker thread count
K       = max search keys per candidate
V       = max query variants
S       = average total searchable key length per candidate
C_lang  = number of candidates with language-specific keys
L_lang  = average language-specific key length per candidate
B       = number of candidates receiving high-quality scoring
H       = number of matched candidates
R_lang(L) = reading/key generation cost for the selected language
h       = persistent cache hit rate
```

Important caps:

```text
K <= max_search_keys_per_candidate
V <= max_query_variants
S <= max_total_key_bytes_per_candidate
B <= top_b_for_quality_score
```

### 23.2 Preprocessing / indexing cost

Plain indexing:

```text
O(NL)
```

Language-specific key generation without cache:

```text
O(NL + C_lang * R_lang(L))
```

Language-specific key generation with persistent cache:

```text
O(NL + (1 - h) * C_lang * R_lang(L))
```

If key generation is lazy, startup can be closer to:

```text
O(NL)
```

with reading generation moved to worker/cache maintenance.

### 23.3 Query expansion cost

Unbounded ambiguous reverse mapping can be exponential:

```text
O(A^q)
```

where `A` is the average branching factor.

Yuru must use caps/beam search:

```text
O(Vq)
```

Since `V` is capped, this is effectively small compared with scanning `N` candidates.

### 23.4 Per-keypress search cost

Naive all-variants x all-keys matching:

```text
O(N * V * S * q / T)      # DP/high-quality matcher
O(N * V * S / T)          # greedy/subsequence matcher
```

This is the design to avoid.

Key-kind filtered design:

```text
O((N * L * q + C_lang * V_lang * L_lang * q) / T)
```

For greedy/subsequence scoring:

```text
O((N * L + C_lang * V_lang * L_lang) / T)
```

If Stage 2 high-quality scoring is only applied to top `B` candidates:

```text
Stage 1: O((N * L + C_lang * V_lang * L_lang) / T)
Stage 2: O(B * V * L_lang * q / T)
```

If `B`, `V`, `K`, and max key bytes are capped, Stage 2 is bounded by a constant relative to `N`.

### 23.5 Sorting / top-k cost

Full sort of matched candidates:

```text
O(H log H)
```

Top-k heap for display limit `M`:

```text
O(H log M)
```

Recommendation:

```text
Use top-k heap or partial selection when H is large.
For MVP, full sort is acceptable, but keep ranking code isolated.
```

### 23.6 Memory complexity

Baseline fzf-like storage:

```text
O(NL)
```

Yuru with generated keys:

```text
O(NL + N*S)
```

With language-specific keys only for matching candidates:

```text
O(NL + C_lang * L_lang)
```

With caps:

```text
O(N * min(S, max_total_key_bytes_per_candidate))
```

Cache storage:

```text
O(U * S_cached)
```

where `U` is the number of unique cached candidate strings or file identities.

### 23.7 fzf comparison

fzf-style greedy baseline:

```text
O(NL / T)
```

fzf-style DP/high-quality baseline:

```text
O(NLq / T)
```

Yuru plain mode should be approximately the same order as fzf-like matching:

```text
O(NL / T) or O(NLq / T)
```

Yuru language mode with key-kind filtering:

```text
O((N * L * q + C_lang * V_lang * L_lang * q) / T)
```

Relative to fzf DP baseline:

```text
1 + (C_lang / N) * V_lang * (L_lang / L)
```

Naive language mode without filtering:

```text
O(N * V * S * q / T)
```

Relative to fzf DP baseline:

```text
V * S / L
```

### 23.8 Example estimate

Assume:

```text
N = 100,000
L = 40
q = 5
T = 8
C_lang / N = 0.2
V_lang = 4
L_lang = 80
B = 1,000
```

fzf-like DP baseline work:

```text
N * L * q = 100,000 * 40 * 5 = 20,000,000 cell-ish operations
per 8 workers: ~2,500,000 units
```

Yuru with key-kind filtering:

```text
original work:
  100,000 * 40 * 5 = 20,000,000

language work:
  20,000 * 4 * 80 * 5 = 32,000,000

total:
  52,000,000

relative to baseline:
  52M / 20M = 2.6x
```

Naive all-key/all-variant matching with `S = 120`, `V = 4`:

```text
100,000 * 4 * 120 * 5 = 240,000,000
relative to baseline:
  240M / 20M = 12x
```

Stage 2 top-B extra:

```text
B * V * L_lang * q = 1,000 * 4 * 80 * 5 = 1,600,000
per 8 workers: ~200,000 units
```

So the target architecture is roughly:

```text
2-4x fzf-like work in language mode, not 10-20x.
```

Actual wall time depends heavily on allocation, Unicode handling, cache locality, scorer constants, and thread scheduling.

### 23.9 Cumulative cost while typing

If user types `q` characters and Yuru reruns search after each keypress, DP-like cumulative matcher cost is roughly:

```text
O(NL * (1 + 2 + ... + q) / T)
= O(NLq^2 / T)
```

If reading generation accidentally runs on every keypress:

```text
O(q * N * R_lang(L) + NLq^2 / T)
```

This is unacceptable. Use the hot-path test from section 22.10 to prevent regressions.

### 23.10 Big-O acceptance criteria

Codex should implement counters and tests that enforce:

```text
- search() does not call build_candidate_keys()
- search() does not call Japanese/Chinese reading generation
- query variants are capped
- candidate keys are capped
- top-B quality scoring is capped
- language backends do not cross-generate other languages' keys
```

---

## 24. Codex Addendum: Candidate Crates and Algorithms

### 24.1 Recommended initial architecture

Start with traits so the matcher and language processors are swappable:

```rust
pub trait MatcherBackend {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64>;
}

pub trait LanguageBackend {
    fn mode(&self) -> LangMode;
    fn normalize_candidate(&self, text: &str) -> String;
    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey>;
    fn expand_query(&self, query: &str) -> Vec<QueryVariant>;
}
```

MVP recommendation:

```text
1. Implement a small custom greedy subsequence matcher first.
2. Keep a MatcherBackend trait.
3. Add a nucleo backend behind a feature flag or experiment module.
4. Benchmark custom matcher vs nucleo vs skim/ib-matcher direct matching.
```

Reason: the data model, query expansion, caps, and key-kind filtering are the core project risk. A tiny matcher is enough to validate those first.

### 24.2 Algorithm matrix

| Layer | Algorithm | Big-O search cost | Use in Yuru | Notes |
|---|---:|---:|---|---|
| Fast fuzzy | Greedy subsequence scan | `O(L)` per key | MVP default or Stage 1 | Simple, predictable, close to fzf v1 spirit. |
| High-quality fuzzy | Smith-Waterman-like DP | `O(Lq)` per key | Stage 2 top-B only | Better ranking, too expensive for all keys. |
| Exact prefilter | substring search | `O(L)` | Stage 0 | Use before fuzzy when helpful. |
| Multi-token prefilter | Aho-Corasick | `O(L + matches)` after automaton build | Optional | Best when many fixed patterns are searched together. |
| Regex mode | finite automata regex | commonly `O(mn)` worst case | Optional `--regex` | Keep separate from default fuzzy. |
| Edit distance | Levenshtein DP | `O(Lq)` | Not default | Typo tolerant, different from fzf semantics. |
| Indexed fuzzy | Levenshtein automata / inverted index | index-dependent | Future huge index mode | Tantivy/Lucene-like; not stdin-first fzf mode. |
| Japanese query expansion | trie / longest match + bounded branching | `O(Vq)` | Required for ja | Must cap variants. |
| Chinese pinyin generation | dictionary lookup + capped variants | `O(L)` to `O(beam * L)` | Required for zh | Avoid Cartesian explosion for polyphones. |

### 24.3 Rust crate matrix

| Area | Crate | Initial decision | Why / role |
|---|---|---|---|
| Core fuzzy finder | `nucleo` | Evaluate seriously | High-level fzf/skim-like fuzzy matcher designed for TUI use, background worker, snapshots, concurrent input. |
| Low-level fuzzy matcher | `nucleo-matcher` | Reference/evaluate, not direct UI loop initially | Docs recommend high-level `nucleo` for interactive finders with more than ~100 items. |
| Existing Rust fzf-like tool | `skim` | Benchmark/reference | Library + CLI similar to fzf. Useful comparison target. |
| Simple Rust fuzzy matcher | `fuzzy-matcher` | Reference/test baseline | Smith-Waterman-like Rust matcher; easy to compare. |
| High-performance experimental matcher | `ncp-matcher`, `frizbee` | Optional investigation | Possible future benchmark candidates. Not MVP dependency. |
| Byte/substring prefilter | `memchr` | Use early | Optimized byte and substring search primitives; good Stage 0 filter. |
| Multiple fixed patterns | `aho-corasick` | Optional | Useful for multi-token or multi-alias prefilter. |
| Compact Aho-Corasick | `daachorse` | Optional | Alternative for large pattern sets. |
| Regex mode | `regex` | Use for simple `--regex` | Safer default regex API. |
| Regex internals | `regex-automata` | Advanced optional | Expert API, multi-pattern/automata controls. |
| Unicode normalization | `unicode-normalization` | Use | NFKC/NFC/lowercase normalization. |
| Grapheme segmentation | `unicode-segmentation` | Use if highlighting must be grapheme-correct | Needed for robust Unicode highlight positions. |
| Japanese kana/romaji | `wana_kana` | Compare, maybe use for simple conversions | Converts/checks Japanese chars, kana, romaji. Still need Yuru-specific variant caps. |
| Japanese kana/romaji | `romkan` | Compare | Simple romaji/kana conversion. |
| Japanese character cleanup | `kana` / `unicode-jp` | Optional | Half-width kana and full-width alphanumeric conversion. |
| Japanese heavy reading | `vibrato` | Future plugin | Viterbi-based tokenization/morphological analysis. Keep outside hot path. |
| Japanese heavy reading | `lindera` | Future plugin | Morphological analysis. Useful but heavier than simple query expansion. |
| Japanese tokenizer | `vaporetto` | Future plugin | Fast/light tokenizer; requires model. |
| Multilingual pinyin/romaji matching | `ib-matcher` | Compare/reference | Directly supports Chinese pinyin and Japanese romaji matching. Evaluate latency/ranking fit. |
| Chinese pinyin matching | `ib-pinyin` | Compare/reference | Supports pinyin schemes, polyphones, mixed notation. Useful for zh backend. |
| Chinese pinyin conversion | `pinyin` | Candidate for key generation | Character and multi-reading pinyin APIs. |
| Cache | `redb` | Good embedded option | Pure Rust embedded DB; good candidate for cache. |
| Cache | `rusqlite` | Good practical option | Stable, inspectable cache. Requires SQLite. |
| Cache | `sled` | Optional | Embedded KV, but evaluate maintenance/perf. |
| Serialization | `serde`, `bincode`, `rmp-serde` | Use as needed | Cache values/config. |
| Benchmarks | `criterion` or `divan` | Use one | Criterion is common; Divan is lightweight. |
| CLI | `clap` | Use | Parse `--lang`, `--query`, caps, cache flags. |
| Parallelism | `rayon` | Use for custom matcher | Easy parallel scan for prototype. If using nucleo high-level, use nucleo's worker model. |

### 24.4 Non-Rust references

| Project | Language | Why it matters |
|---|---|---|
| `fzf` | Go | Main behavioral inspiration. v1 greedy and v2 Smith-Waterman-like scoring are reference points. |
| `fzy` | C | Small/simple fuzzy ranking reference. Good for building a compact custom scorer. |
| RapidFuzz | C++/Python | Strong edit-distance/typo-tolerant reference, but not fzf-style subsequence matching. |
| Fuse.js | JavaScript | Bitap-style typo tolerant search. Useful conceptually, not default for Yuru. |
| Lucene | Java | Indexed fuzzy term search reference. Relevant only for future persistent index mode. |

### 24.5 Recommended Cargo feature layout

```toml
[features]
default = ["plain", "ja-lite", "zh-lite"]
plain = []
ja-lite = ["dep:unicode-normalization"]
ja-wanakana = ["dep:wana_kana"]
ja-romkan = ["dep:romkan"]
ja-tokenizer-vibrato = ["dep:vibrato"]
ja-tokenizer-lindera = ["dep:lindera"]
zh-lite = ["dep:pinyin"]
zh-ib-pinyin = ["dep:ib-pinyin"]
matcher-nucleo = ["dep:nucleo"]
matcher-skim = ["dep:skim"]
regex-mode = ["dep:regex"]
prefilter-aho = ["dep:aho-corasick"]
cache-redb = ["dep:redb"]
cache-sqlite = ["dep:rusqlite"]
```

MVP should keep dependency count small:

```text
Required MVP dependencies:
  clap
  anyhow
  unicode-normalization
  memchr
  rayon, optional but useful
  serde, optional for config/cache

MVP dev dependencies:
  assert_cmd
  predicates
  tempfile
  proptest
  criterion or divan
```

Add `nucleo`, `ib-matcher`, `ib-pinyin`, `vibrato`, `lindera`, etc. as experiments or optional features after the core data model is stable.

### 24.6 Algorithm selection for MVP

MVP matcher should be deliberately simple:

```text
Input:
  query variant text
  search key text

Return:
  None if query is not an ordered subsequence of key
  Some(score) otherwise

Score bonuses:
  exact match
  prefix match
  consecutive runs
  word/path boundary
  basename match
  shorter span
  lower gap penalty
  key kind weight
  query variant weight
```

Approximate score structure:

```rust
score = 0
score += key_kind_weight
score += query_variant_weight
score += exact_bonus
score += prefix_bonus
score += boundary_bonus
score += consecutive_bonus
score -= gap_penalty
score -= span_penalty
```

Keep this scoring stable enough for relative ranking tests, but do not freeze exact numeric values too early.

### 24.7 When to use nucleo

Evaluate `nucleo` when:

```text
- The custom matcher + rayon is not fast enough.
- TUI integration becomes the main focus.
- You want background worker/snapshot behavior.
- Unicode highlighting correctness matters.
```

Questions to answer in the nucleo spike:

```text
- Can one display item expose multiple searchable columns/keys naturally?
- Can Yuru apply per-key weights cleanly?
- Can language-specific query variants be represented as multiple patterns?
- Does nucleo's ranking remain intuitive for Yuru reading keys?
- Is update/injection suitable for lazy reading generation?
```

### 24.8 When to use ib-matcher / ib-pinyin

Evaluate `ib-matcher` and `ib-pinyin` when:

```text
- Japanese romaji matching quality becomes the bottleneck.
- Chinese polyphone/pinyin matching quality becomes the bottleneck.
- You need a reference for `n'`, `nn`, Hepburn/IME variants, pinyin initials, shuangpin, or mixed notation.
```

Do not assume these libraries solve Yuru's ranking problem. They may be best used as:

```text
- reference implementations
- candidate key generators
- direct-match baselines
- optional language backend features
```

### 24.9 Algorithm/crate evaluation tasks for Codex

Create benchmark subcommands or examples:

```bash
yuru-bench --backend custom --lang ja --query tokyo --dataset fixtures/100k_mixed.txt
yuru-bench --backend nucleo --lang ja --query tokyo --dataset fixtures/100k_mixed.txt
yuru-bench --backend ib-matcher --lang ja --query tokyo --dataset fixtures/100k_mixed.txt
yuru-bench --backend custom --lang zh --query bjdx --dataset fixtures/100k_mixed.txt
yuru-bench --backend ib-pinyin --lang zh --query bjdx --dataset fixtures/100k_mixed.txt
```

Record:

```text
- p50/p95/p99 latency
- result top-10
- fuzzy_calls
- keys_seen
- allocations if measurable
- memory usage if measurable
```

### 24.10 Reference links for crate/algorithm triage

These are starting points for Codex/research; verify exact versions in `Cargo.toml` when implementing.

```text
nucleo:
  https://docs.rs/nucleo/latest/nucleo/

nucleo-matcher:
  https://docs.rs/nucleo-matcher/latest/nucleo_matcher/

skim:
  https://docs.rs/skim/latest/skim/

fzf algorithm source:
  https://github.com/junegunn/fzf/blob/master/src/algo/algo.go

fuzzy-matcher:
  https://github.com/skim-rs/fuzzy-matcher

ib-matcher:
  https://github.com/Chaoses-Ib/ib-matcher/blob/master/ib-matcher/README.md

ib-pinyin:
  https://docs.rs/ib-pinyin/latest/ib_pinyin/

pinyin:
  https://docs.rs/pinyin/latest/pinyin/

memchr:
  https://docs.rs/memchr/latest/memchr/

aho-corasick:
  https://docs.rs/aho-corasick/latest/aho_corasick/

regex-automata:
  https://docs.rs/regex-automata/latest/regex_automata/

wana_kana:
  https://docs.rs/wana_kana/latest/wana_kana/

romkan:
  https://docs.rs/crate/romkan/latest

vibrato:
  https://docs.rs/vibrato/latest/vibrato/

lindera:
  https://github.com/lindera/lindera

vaporetto:
  https://docs.rs/vaporetto/latest/vaporetto/

tantivy FuzzyTermQuery:
  https://docs.rs/tantivy/latest/tantivy/query/struct.FuzzyTermQuery.html
```

---

## 25. Updated First Codex Task

Create a Rust CLI prototype named `yuru` with tests first.

Required behavior:

```text
- CLI: yuru --lang plain|ja|zh --query <QUERY> --limit <N>
- Read candidates from stdin.
- Build Candidate/SearchKey data model.
- Always include Original and Normalized keys.
- Implement plain fuzzy matching with a simple greedy subsequence scorer.
- Implement Japanese romaji->kana query expansion with capped variants.
- Implement light Japanese kana/katakana normalization.
- Implement Chinese pinyin key generation, either with a crate or a small fixture-backed prototype.
- Enforce hard caps for query variants and search keys.
- Add SearchStats counters.
- Ensure search hot path does not call reading generation.
- Add unit tests and CLI integration tests from section 22.
- Add Criterion or Divan benchmark harness from section 22.15.
```

Initial file layout:

```text
Cargo.toml
crates/yuru-core/src/lib.rs
crates/yuru-core/src/normalize.rs
crates/yuru-core/src/candidate.rs
crates/yuru-core/src/query.rs
crates/yuru-core/src/matcher.rs
crates/yuru-core/src/rank.rs
crates/yuru-core/src/stats.rs
crates/yuru-ja/src/lib.rs
crates/yuru-ja/src/romaji.rs
crates/yuru-zh/src/lib.rs
crates/yuru-zh/src/pinyin.rs
crates/yuru/src/main.rs
tests/cli.rs
tests/fixtures/mixed_paths.txt
benches/search.rs
```

Acceptance checks:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo bench --bench search
```

MVP correctness examples:

```bash
printf 'README.md\nsrc/main.rs\n' | yuru --lang plain --query read --limit 1
# => README.md

printf 'カメラ.txt\n' | yuru --lang ja --query kamera --limit 1
# => カメラ.txt

printf '北京大学.txt\n' | yuru --lang zh --query bjdx --limit 1
# => 北京大学.txt
```
