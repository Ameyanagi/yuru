# Yuru

Yuru 是一个快速的命令行 fuzzy finder，支持日文读音、韩文 Hangul 和中文拼音搜索。
它的使用方式接近 fzf，同时针对 CJK 文本提供更准确的 phonetic match 高亮。

## Demo Video

[观看 Yuru command demo](../demo.mp4)

<video src="../demo.mp4" controls muted playsinline width="100%"></video>

## 安装

Yuru 默认安装到用户目录，不需要 `sudo`。

macOS / Linux 交互式安装:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --all --version v0.1.8
```

默认会把 `yuru` 安装到 `~/.local/bin`。可以通过 `XDG_BIN_HOME` 或
`YURU_INSTALL_BIN_DIR` 修改安装目录。这个命令会在交互式终端中询问默认语言、
preview command、preview text extensions、图片 preview protocol、shell binding 和
shell path backend，并写入 `~/.config/yuru/config.toml`。直接按 Enter 会使用各项默认值。
preview command 默认值 `auto` 会在文本预览中优先使用 `bat`，图片则使用内部 preview。
图片 preview protocol 默认值是 `none`。shell path backend 默认值 `auto` 会依次尝试
`fd`、`fdfind` 和 fallback。

显式设置交互式安装的默认值:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --all --version v0.1.8 --default-lang none --preview-command auto --preview-image-protocol none --path-backend auto --bindings all
```

之后可以运行 `yuru configure` 重新配置。

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.8"
```

这会把 `yuru.exe` 安装到 `%LOCALAPPDATA%\Yuru\bin`，更新用户 PATH，并加入 PowerShell profile。
交互环境中会询问默认语言、preview command、preview text extensions、图片 preview protocol、shell binding 和 shell path backend。可以使用 `-DefaultLang none`、`-PreviewCommand auto`、`-PreviewImageProtocol none`、`-PathBackend auto` 或 `-Bindings all` 显式设置交互式安装的默认值。

只安装二进制文件:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.8/install | sh -s -- --version v0.1.8
```

从 crates.io 安装:

```sh
cargo install yuru
```

crates.io package 名称和安装后的命令都是 `yuru`。
源码构建会使用 Lindera embedded IPADIC 来生成日文读音，因此需要 C compiler。
macOS 请安装 Xcode Command Line Tools；仓库的 Cargo config 和脚本会在 Apple target 上优先使用
`/usr/bin/clang`。GitHub release 的预编译二进制不需要本地 compiler。

更多信息见 [install / uninstall docs](install-uninstall.md)。

## Shell 集成

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
yuru --powershell | Invoke-Expression
```

可用快捷键:

- `CTRL-T`: 选择文件或目录并插入到命令行
- `CTRL-R`: 搜索命令历史
- `ALT-C`: 进入选择的目录
- `**<TAB>`: fuzzy path completion

## 使用示例

中文拼音首字母:

```sh
printf "北京大学.txt\nnotes.txt\n" | yuru --lang zh --filter bjdx
```

日文 romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yuru --lang ja --filter kamera
```

韩文 Hangul romanization / 初声 / 2-set keyboard:

```sh
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter hangeul
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter ㅎㄱ
printf "한글.txt\nnotes.txt\n" | yuru --lang ko --filter gksrmf
```

文件搜索:

```sh
fd --hidden --exclude .git . | yuru --scheme path
```

## fzf 兼容和配置

Yuru 可以解析 fzf 的主要 option surface，因此现有 shell binding 和 `FZF_DEFAULT_OPTS` 不容易因为解析失败而中断。`--filter`、`--query`、`--read0`、`--print0`、`--nth`、`--with-nth`、`--scheme`、`--walker`、`--expect` 已实现。`--bind` 仍是子集支持，未支持的 action 默认会输出 warning。

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

如果 preview command 输出图片 bytes，Yuru 会通过 `ratatui-image` 渲染。需要时可用
`YURU_PREVIEW_IMAGE_PROTOCOL=sixel|kitty|iterm2|halfblocks` 固定协议。
图片 preview 由默认启用的 `image` feature 提供。如需更小的源码构建，可使用
`cargo install yuru --no-default-features`。

`~/.config/yuru/config.toml` 可以设置 `lang = "auto"`、`load_fzf_defaults = "safe"`、`algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`、`[ja] reading = "none" | "lindera"`、`[ko] initials = true`、`[zh] initials = true` 等。CLI 参数优先级最高。

详细兼容性见 [fzf compatibility](fzf-compat.md)，语言匹配行为见 [language matching](language-matching.md)。

## 开发

```sh
./scripts/install-hooks
./scripts/check
./scripts/bench
YURU_BENCH_1M=1 ./scripts/bench
```

git hook 会运行 formatter、linter、测试和 benchmark。只有在确实需要快速本地提交时才使用
`YURU_SKIP_BENCH=1`。

## 发布

push version tag 后，GitHub Actions 会生成 macOS、Linux、Windows 的 release assets，并发布到 crates.io。
release workflow 只会在 tag push 时运行，tag 必须和 crate version 一致。

```sh
git tag v0.1.8
git push origin v0.1.8
```

## 许可证

Yuru 同时按照 MIT license 和 Apache License 2.0 的条款发布。请参阅
[LICENSE-MIT](../LICENSE-MIT) 和 [LICENSE-APACHE](../LICENSE-APACHE)。
