# Yuru

Yuru は、日本語と中国語の音読み検索に対応した高速なコマンドライン fuzzy finder です。
fzf に近い操作感を保ちながら、CJK テキストの phonetic match と正確なハイライトを重視しています。

## インストール

Yuru はデフォルトでユーザー領域にインストールされます。`sudo` は不要です。

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install | sh -s -- --all --version v0.1.4
```

通常は `~/.local/bin` に `yuru` を配置します。`XDG_BIN_HOME` または
`YURU_INSTALL_BIN_DIR` を設定すると変更できます。`--all` を付けると現在の shell の設定にも統合を追加します。
インストーラーはデフォルト言語を尋ね、`~/.config/yuru/config.toml` に保存します。

installer は既定で `ja` を `~/.config/yuru/config.toml` に書きます。プロンプトなしで言語や key binding を指定する場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install | sh -s -- --all --version v0.1.4 --default-lang ja --bindings ask
```

あとから変更する場合は `yuru configure` を実行します。

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.4"
```

`%LOCALAPPDATA%\Yuru\bin` に `yuru.exe` を配置し、ユーザー PATH と PowerShell profile を更新します。
`-DefaultLang ja` や `-Bindings ask` のように指定すると `%APPDATA%\yuru\config.toml` に既定値を書き込みます。

バイナリだけを入れる場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install | sh -s -- --version v0.1.4
```

crates.io から入れる場合:

```sh
cargo install yuru
```

crates.io package 名とインストールされるコマンド名はどちらも `yuru` です。
source build では日本語読みのために Lindera embedded IPADIC を使うので、C compiler が必要です。
macOS では Xcode Command Line Tools を入れてください。repo の Cargo config と scripts は Apple target で
`/usr/bin/clang` を優先します。GitHub release の binary はローカル compiler 不要です。

詳細は [install / uninstall docs](install-uninstall.md) を参照してください。

## Shell 統合

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
yuru --powershell | Invoke-Expression
```

利用できる操作:

- `CTRL-T`: ファイル / ディレクトリを選択してコマンドラインへ挿入
- `CTRL-R`: 履歴検索
- `ALT-C`: 選択したディレクトリへ移動
- `**<TAB>`: fuzzy path completion

## 使い方

日本語 romaji 検索:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yuru --lang ja --filter kamera
```

中国語 pinyin initials:

```sh
printf "北京大学.txt\nnotes.txt\n" | yuru --lang zh --filter bjdx
```

ファイル検索:

```sh
fd --hidden --exclude .git . | yuru --scheme path
```

## fzf 互換と設定

Yuru は fzf の主要な option surface を parse できるため、既存の shell binding や `FZF_DEFAULT_OPTS` が parse error になりにくくなっています。`--filter`、`--query`、`--read0`、`--print0`、`--nth`、`--with-nth`、`--scheme`、`--walker`、`--expect` は実装済みです。`--bind` は subset 対応で、未対応の action は既定で warning になります。

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

preview command が画像 bytes を出す場合は `ratatui-image` で描画します。必要なら
`YURU_PREVIEW_IMAGE_PROTOCOL=sixel|kitty|iterm2|halfblocks` で protocol を固定できます。
画像 preview は既定の `image` feature で有効です。source build を軽くしたい場合は
`cargo install yuru --no-default-features` を使えます。

`~/.config/yuru/config.toml` では `lang = "auto"`、`load_fzf_defaults = "safe"`、`algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`、`[ja] reading = "none" | "lindera"`、`[zh] initials = true` などを設定できます。CLI 引数が最優先です。

詳しい互換性は [fzf compatibility](fzf-compat.md)、言語ごとの挙動は [language matching](language-matching.md) を参照してください。

## 開発

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
```

git hook は formatter、linter、test、benchmark を実行します。ローカルで一時的に benchmark を飛ばす場合だけ
`YURU_SKIP_BENCH=1` を使ってください。

## リリース

version tag を push すると GitHub Actions が macOS、Linux、Windows 向けの release asset を作成し、crates.io に publish します。
release workflow は tag push でだけ動き、tag は crate version と一致している必要があります。

```sh
git tag v0.1.4
git push origin v0.1.4
```

## ライセンス

Yuru は MIT license と Apache License 2.0 の両方の条件で配布されます。
[LICENSE-MIT](../LICENSE-MIT) と [LICENSE-APACHE](../LICENSE-APACHE) を参照してください。
