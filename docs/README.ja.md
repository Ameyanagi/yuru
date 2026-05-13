# Yuru

Yuru は、日本語の読み、韓国語のハングル、中国語のピンインで検索できる高速なコマンドライン fuzzy finder です。
fzf に近い操作感を保ちながら、CJK テキストを音で探しやすくし、元の文字列を正確にハイライトすることを重視しています。

## デモ動画

[Yuru のコマンドデモを見る](../demo.mp4)

<video src="../demo.mp4" controls muted playsinline width="100%"></video>

## インストール

Yuru は標準ではユーザー領域にインストールされるため、`sudo` は不要です。

macOS / Linux の対話式インストール:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --all --version v0.1.10
```

通常は `~/.local/bin` に `yuru` を配置します。`XDG_BIN_HOME` または
`YURU_INSTALL_BIN_DIR` を設定するとインストール先を変更できます。このコマンドを対話式ターミナルで実行すると、
既定の言語、プレビューコマンド、プレビュー対象のテキスト拡張子、画像プレビュープロトコル、
シェルバインド、シェルのパス検索バックエンドを尋ね、`~/.config/yuru/config.toml` に保存します。
各プロンプトで Enter を押すと、その項目の既定値が使われます。プレビューコマンドの既定値 `auto` は、
テキストでは利用できる場合に `bat` を使い、画像では Yuru 内蔵のプレビューを使います。
画像プレビュープロトコルの既定値は `none` です。シェルのパス検索バックエンドの既定値 `auto` は
`fd`、`fdfind`、ポータブルなフォールバックの順に試します。

日本語を既定の検索言語にして、対話式インストールの値も明示する例:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --all --version v0.1.10 --default-lang ja --preview-command auto --preview-image-protocol none --path-backend auto --bindings all
```

あとから設定を変更する場合は `yuru configure` を実行します。

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.10"
```

`%LOCALAPPDATA%\Yuru\bin` に `yuru.exe` を配置し、ユーザー PATH と PowerShell profile を更新します。
対話式環境では、既定の言語、プレビューコマンド、プレビュー対象のテキスト拡張子、画像プレビュープロトコル、シェルバインド、シェルのパス検索バックエンドを尋ねます。
日本語を既定にして値を明示する場合は、次のように指定します。

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.10 -DefaultLang ja -PreviewCommand auto -PreviewImageProtocol none -PathBackend auto -Bindings all"
```

バイナリだけをインストールする場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.10/install | sh -s -- --version v0.1.10
```

crates.io からインストールする場合:

```sh
cargo install yuru
```

crates.io のパッケージ名と、インストールされるコマンド名はいずれも `yuru` です。
ソースからビルドする場合、日本語の読みを生成するために Lindera embedded IPADIC を使うため、C コンパイラが必要です。
macOS では Xcode Command Line Tools をインストールしてください。このリポジトリの Cargo config と scripts は Apple ターゲットで
`/usr/bin/clang` を優先します。GitHub release のビルド済みバイナリはローカルのコンパイラを必要としません。

詳しくは [install / uninstall docs](install-uninstall.md) を参照してください。

## シェル連携

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
Invoke-Expression ((yuru --powershell) -join "`n")
```

利用できる操作:

- `CTRL-T`: ファイルまたはディレクトリを選択してコマンドラインに挿入
- `CTRL-R`: コマンド履歴を検索
- `ALT-C`: 選択したディレクトリへ移動
- `**<TAB>`: fuzzy path completion

## 使い方

日本語の romaji 検索:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yuru --lang ja --filter kamera
```

中国語の pinyin initials:

```sh
printf "北京大学.txt\nnotes.txt\n" | yuru --lang zh --filter bjdx
```

韓国語の Hangul romanization / 初声 / 2-set keyboard:

```sh
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter hangeul
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter ㅎㄱ
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter gksrmf
```

ファイル検索:

```sh
fd --hidden --exclude .git . | yuru --scheme path
```

## fzf 互換性と設定

Yuru は fzf の主要なオプション群を解釈できるため、既存のシェルバインドや `FZF_DEFAULT_OPTS` が解析エラーで止まりにくくなっています。
`--filter`、`--query`、`--read0`、`--print0`、`--nth`、`--with-nth`、`--scheme`、`--walker`、`--expect` は実装済みです。
`--bind` は一部対応で、未対応のアクションは既定で警告になります。

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

`preview.image_protocol = "auto"` のとき、プレビューコマンドが画像のバイト列を出力する場合は `ratatui-image` で描画します。必要に応じて
`YURU_PREVIEW_IMAGE_PROTOCOL=sixel|kitty|iterm2|halfblocks` でプロトコルを固定できます。`none` では画像を描画せず、形式などの短い情報だけを表示します。
画像プレビューは既定の `image` feature で有効です。ソースビルドを軽くしたい場合は
`cargo install yuru --no-default-features` を使えます。

日本語を既定にするには、`~/.config/yuru/config.toml` に `lang = "ja"` を設定します。
日本語、韓国語、中国語を同じ候補リストで同時に検索したい場合は `lang = "all"` を使います。
ほかにも `lang = "auto"`、`load_fzf_defaults = "safe"`、`algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`、
`[ja] reading = "none" | "lindera"`、`[ko] initials = true`、`[zh] initials = true` などを設定できます。
CLI 引数が最優先です。

詳しい互換性は [fzf compatibility](fzf-compat.md)、言語ごとの挙動は [language matching](language-matching.md) を参照してください。

## 開発

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
```

git hook は formatter、linter、test、benchmark を実行します。ローカルで一時的に benchmark を省略したい場合にだけ
`YURU_SKIP_BENCH=1` を使ってください。

## リリース

version tag を push すると、GitHub Actions が macOS、Linux、Windows 向けのリリースアセットを作成し、crates.io に公開します。
release workflow は tag push のときだけ動き、tag は crate version と一致している必要があります。

```sh
git tag v0.1.10
git push origin v0.1.10
```

## ライセンス

Yuru は MIT ライセンスと Apache License 2.0 の両方の条件で配布されます。
[LICENSE-MIT](../LICENSE-MIT) と [LICENSE-APACHE](../LICENSE-APACHE) を参照してください。
