# Yuru

Yuru is a fast command-line fuzzy finder with Japanese and Chinese phonetic search.
It is designed to feel familiar to fzf users while adding multilingual matching and
source-span highlighting for CJK text.

Localized documentation:

- [日本語](docs/README.ja.md)
- [中文](docs/README.zh.md)
- [한국어](docs/README.ko.md)

## Install

Yuru installs into user space by default. It does not require `sudo`.

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh -s -- --all
```

This installs `yuru` into `~/.local/bin` unless `XDG_BIN_HOME` or
`YURU_INSTALL_BIN_DIR` is set. `--all` also adds shell integration for the current
shell. The installer asks for a default language and writes it to
`~/.config/yuru/config`.

To set the default language without a prompt:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh -s -- --all --default-lang ja
```

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/main/install.ps1
Invoke-Expression "& { $script } -All"
```

This installs `yuru.exe` into `%LOCALAPPDATA%\Yuru\bin`, adds that directory to
the user PATH, adds PowerShell integration to your user profile, and can write
the default language to `%APPDATA%\yuru\config`.

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/main/install.ps1
Invoke-Expression "& { $script } -All -DefaultLang ja"
```

To install only the binary:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/main/install | sh
```

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/main/install.ps1
Invoke-Expression "& { $script }"
```

Crates.io:

```sh
cargo install yuru
```

The crates.io package and installed command are both `yuru`.

## Shell Integration

Yuru can print shell setup code directly from the binary:

```sh
eval "$(yuru --bash)"
source <(yuru --zsh)
yuru --fish | source
```

PowerShell:

```powershell
yuru --powershell | Invoke-Expression
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
printf "README.md\nsrc/lib.rs\ntests/日本語.txt\n" | yuru --lang ja --filter ni
```

Open the interactive finder:

```sh
yuru --walker file,dir,follow,hidden --scheme path
```

Chinese pinyin initials:

```sh
printf "北京大学.txt\nnotes.txt\n" | yuru --lang zh --filter bjdx
```

Japanese romaji:

```sh
printf "カメラ.txt\ntests/日本人の.txt\n" | yuru --lang ja --filter kamera
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
YURU_BENCH_1M=1 ./scripts/bench
```

The hook policy runs formatter, linter, tests, and benchmarks before commits and
pushes. Set `YURU_SKIP_BENCH=1` only when you intentionally need a fast local
checkpoint.

## Releases

GitHub Actions builds release assets for:

- `x86_64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Create a version tag to publish a release and crates.io packages. The release
workflow only runs on tags, and the tag must match the crate version.

```sh
git tag v0.1.1
git push origin v0.1.1
```

## License

Yuru is distributed under the terms of both the MIT license and the Apache
License 2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
