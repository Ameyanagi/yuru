# Yuru

Yuru 是一个快速的命令行 fuzzy finder，支持日文读音搜索和中文拼音搜索。
它的使用方式接近 fzf，同时针对 CJK 文本提供更准确的 phonetic match 高亮。

## 安装

Yuru 默认安装到用户目录，不需要 `sudo`。

macOS / Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.3/install | sh -s -- --all --version v0.1.3
```

默认会把 `yuru` 安装到 `~/.local/bin`。可以通过 `XDG_BIN_HOME` 或
`YURU_INSTALL_BIN_DIR` 修改安装目录。`--all` 会为当前 shell 添加集成配置。
安装器会询问默认语言，并写入 `~/.config/yuru/config.toml`。

无需提示直接指定默认语言:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.3/install | sh -s -- --all --version v0.1.3 --default-lang zh
```

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.3/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.3"
```

这会把 `yuru.exe` 安装到 `%LOCALAPPDATA%\Yuru\bin`，更新用户 PATH，并加入 PowerShell profile。
可以使用 `-DefaultLang zh` 写入 `%APPDATA%\yuru\config.toml`。

只安装二进制文件:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.3/install | sh -s -- --version v0.1.3
```

从 crates.io 安装:

```sh
cargo install yuru
```

crates.io package 名称和安装后的命令都是 `yuru`。

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

文件搜索:

```sh
yuru --walker file,dir,follow,hidden --scheme path
```

## fzf 兼容和配置

Yuru 支持 `--filter`、`--query`、`--read0`、`--print0`、`--nth`、`--with-nth`、`--scheme`、`--walker`、`--expect`，以及 `accept` / `abort` / `clear-query` 的 `--bind` 子集。尚未支持的 fzf 选项默认会输出 warning。

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

`~/.config/yuru/config.toml` 可以设置 `lang = "auto"`、`load_fzf_defaults = "safe"`、`algo = "greedy" | "fzf-v1" | "fzf-v2" | "nucleo"`、`[ja] reading = "none" | "lindera"`、`[zh] initials = true` 等。CLI 参数优先级最高。

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
git tag v0.1.3
git push origin v0.1.3
```

## 许可证

Yuru 同时按照 MIT license 和 Apache License 2.0 的条款发布。请参阅
[LICENSE-MIT](../LICENSE-MIT) 和 [LICENSE-APACHE](../LICENSE-APACHE)。
