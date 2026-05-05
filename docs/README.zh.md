# Yomi

Yomi 是一个快速的命令行 fuzzy finder，支持日文读音搜索和中文拼音搜索。
它的使用方式接近 fzf，同时针对 CJK 文本提供更准确的 phonetic match 高亮。

## 安装

Yomi 默认安装到用户目录，不需要 `sudo`。

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh -s -- --all
```

默认会把 `yomi` 安装到 `~/.local/bin`。可以通过 `XDG_BIN_HOME` 或
`YOMI_INSTALL_BIN_DIR` 修改安装目录。`--all` 会为当前 shell 添加集成配置。

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yomi/main/install.ps1
Invoke-Expression "& { $script } -All"
```

这会把 `yomi.exe` 安装到 `%LOCALAPPDATA%\Yomi\bin`，更新用户 PATH，并加入 PowerShell profile。

只安装二进制文件:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh
```

## Shell 集成

```sh
eval "$(yomi --bash)"
source <(yomi --zsh)
yomi --fish | source
```

PowerShell:

```powershell
yomi --powershell | Invoke-Expression
```

可用快捷键:

- `CTRL-T`: 选择文件或目录并插入到命令行
- `CTRL-R`: 搜索命令历史
- `ALT-C`: 进入选择的目录
- `**<TAB>`: fuzzy path completion

## 使用示例

中文拼音首字母:

```sh
printf "北京大学.txt\nnotes.txt\n" | yomi --lang zh --filter bjdx
```

日文 romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yomi --lang ja --filter kamera
```

文件搜索:

```sh
yomi --walker file,dir,follow,hidden --scheme path
```

## 开发

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YOMI_BENCH_1M=1 ./scripts/bench
```

git hook 会运行 formatter、linter、测试和 benchmark。只有在确实需要快速本地提交时才使用
`YOMI_SKIP_BENCH=1`。

## 发布

push tag 后，GitHub Actions 会生成 macOS、Linux、Windows 的 release assets。

```sh
git tag v0.1.0
git push origin v0.1.0
```
