# Yomi

Yomi は、日本語と中国語の音読み検索に対応した高速なコマンドライン fuzzy finder です。
fzf に近い操作感を保ちながら、CJK テキストの phonetic match と正確なハイライトを重視しています。

## インストール

Yomi はデフォルトでユーザー領域にインストールされます。`sudo` は不要です。

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh -s -- --all
```

通常は `~/.local/bin` に `yomi` を配置します。`XDG_BIN_HOME` または
`YOMI_INSTALL_BIN_DIR` を設定すると変更できます。`--all` を付けると現在の shell の設定にも統合を追加します。

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yomi/main/install.ps1
Invoke-Expression "& { $script } -All"
```

`%LOCALAPPDATA%\Yomi\bin` に `yomi.exe` を配置し、ユーザー PATH と PowerShell profile を更新します。

バイナリだけを入れる場合:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh
```

## Shell 統合

```sh
eval "$(yomi --bash)"
source <(yomi --zsh)
yomi --fish | source
```

PowerShell:

```powershell
yomi --powershell | Invoke-Expression
```

利用できる操作:

- `CTRL-T`: ファイル / ディレクトリを選択してコマンドラインへ挿入
- `CTRL-R`: 履歴検索
- `ALT-C`: 選択したディレクトリへ移動
- `**<TAB>`: fuzzy path completion

## 使い方

日本語 romaji 検索:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yomi --lang ja --filter kamera
```

中国語 pinyin initials:

```sh
printf "北京大学.txt\nnotes.txt\n" | yomi --lang zh --filter bjdx
```

ファイル検索:

```sh
yomi --walker file,dir,follow,hidden --scheme path
```

## 開発

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YOMI_BENCH_1M=1 ./scripts/bench
```

git hook は formatter、linter、test、benchmark を実行します。ローカルで一時的に benchmark を飛ばす場合だけ
`YOMI_SKIP_BENCH=1` を使ってください。

## リリース

タグを push すると GitHub Actions が macOS、Linux、Windows 向けの release asset を作成します。

```sh
git tag v0.1.0
git push origin v0.1.0
```
