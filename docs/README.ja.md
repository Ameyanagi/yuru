# Yuru

Yuru は、日本語と中国語の音読み検索に対応した高速なコマンドライン fuzzy finder です。
fzf に近い操作感を保ちながら、CJK テキストの phonetic match と正確なハイライトを重視しています。

## インストール

Yuru はデフォルトでユーザー領域にインストールされます。`sudo` は不要です。

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh -s -- --all
```

通常は `~/.local/bin` に `yuru` を配置します。`XDG_BIN_HOME` または
`YURU_INSTALL_BIN_DIR` を設定すると変更できます。`--all` を付けると現在の shell の設定にも統合を追加します。
インストーラーはデフォルト言語を尋ね、`~/.config/yuru/config` に保存します。

プロンプトなしでデフォルト言語を指定する場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh -s -- --all --default-lang ja
```

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/main/install.ps1
Invoke-Expression "& { $script } -All"
```

`%LOCALAPPDATA%\Yuru\bin` に `yuru.exe` を配置し、ユーザー PATH と PowerShell profile を更新します。
`-DefaultLang ja` のように指定すると `%APPDATA%\yuru\config` にデフォルト言語を書き込みます。

バイナリだけを入れる場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh
```

crates.io から入れる場合:

```sh
cargo install yuru
```

crates.io package 名とインストールされるコマンド名はどちらも `yuru` です。

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
yuru --walker file,dir,follow,hidden --scheme path
```

## fzf 互換と設定

Yuru は `--filter`、`--query`、`--read0`、`--print0`、`--nth`、`--with-nth`、`--scheme`、`--walker`、`--expect` と、`accept` / `abort` / `clear-query` の `--bind` subset をサポートします。未対応の fzf option は既定で warning になります。

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

`~/.config/yuru/config.toml` では `lang = "auto"`、`load_fzf_defaults = "safe"`、`algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`、`[ja] reading = "none" | "lindera"`、`[zh] initials = true` などを設定できます。CLI 引数が最優先です。

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
git tag v0.1.2
git push origin v0.1.2
```

## ライセンス

Yuru は MIT license と Apache License 2.0 の両方の条件で配布されます。
[LICENSE-MIT](../LICENSE-MIT) と [LICENSE-APACHE](../LICENSE-APACHE) を参照してください。
