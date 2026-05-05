# Yomi

Yomi is a fast command-line fuzzy finder with Japanese and Chinese phonetic search.
It is designed to feel familiar to fzf users while adding multilingual matching and
source-span highlighting for CJK text.

Localized documentation:

- [日本語](docs/README.ja.md)
- [中文](docs/README.zh.md)
- [한국어](docs/README.ko.md)

## Install

Yomi installs into user space by default. It does not require `sudo`.

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh -s -- --all
```

This installs `yomi` into `~/.local/bin` unless `XDG_BIN_HOME` or
`YOMI_INSTALL_BIN_DIR` is set. `--all` also adds shell integration for the current
shell.

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yomi/main/install.ps1
Invoke-Expression "& { $script } -All"
```

This installs `yomi.exe` into `%LOCALAPPDATA%\Yomi\bin`, adds that directory to
the user PATH, and adds PowerShell integration to your user profile.

To install only the binary:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yomi/main/install | sh
```

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yomi/main/install.ps1
Invoke-Expression "& { $script }"
```

## Shell Integration

Yomi can print shell setup code directly from the binary:

```sh
eval "$(yomi --bash)"
source <(yomi --zsh)
yomi --fish | source
```

PowerShell:

```powershell
yomi --powershell | Invoke-Expression
```

The shell integration provides:

- `CTRL-T`: insert selected files or directories
- `CTRL-R`: search command history
- `ALT-C`: cd into a selected directory
- `**<TAB>`: fuzzy path completion

The bash, zsh, and fish behavior follows fzf’s documented shell integration
model. PowerShell support uses PSReadLine key handlers.

## Usage

Filter input:

```sh
printf "README.md\nsrc/lib.rs\ntests/日本語.txt\n" | yomi --lang ja --filter ni
```

Open the interactive finder:

```sh
yomi --walker file,dir,follow,hidden --scheme path
```

Chinese pinyin initials:

```sh
printf "北京大学.txt\nnotes.txt\n" | yomi --lang zh --filter bjdx
```

Japanese romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yomi --lang ja --filter kamera
```

## Development

Install local git hooks:

```sh
./scripts/install-hooks
```

Run the quality gate:

```sh
./scripts/check
```

Run benchmarks:

```sh
./scripts/bench
YOMI_BENCH_1M=1 ./scripts/bench
```

The hook policy runs formatter, linter, tests, and benchmarks before commits and
pushes. Set `YOMI_SKIP_BENCH=1` only when you intentionally need a fast local
checkpoint.

## Releases

GitHub Actions builds release assets for:

- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Create a tag to publish a release:

```sh
git tag v0.1.0
git push origin v0.1.0
```
