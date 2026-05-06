# Install, Update, And Uninstall

Yuru installers are user-space installers. They do not require `sudo`.

## Release-Pinned Install

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install | sh -s -- --all --version v0.1.4
```

Windows PowerShell:

```powershell
$script = Invoke-RestMethod https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install.ps1
Invoke-Expression "& { $script } -All -Version v0.1.4"
```

The Unix installer writes the binary to `~/.local/bin` unless `XDG_BIN_HOME`, `YURU_INSTALL_BIN_DIR`, or `--bin-dir` overrides it. The Windows installer writes to `%LOCALAPPDATA%\Yuru\bin`.

`--all` can also add shell integration and write config. The default language is
Japanese (`ja`) when prompts are empty or unavailable. Use
`--default-lang ask|plain|ja|zh|auto|none` or `-DefaultLang` to override it.
Use `--bindings ask|all|none|ctrl-t,ctrl-r,alt-c,completion` or `-Bindings` to
choose which shell bindings are enabled. `yuru configure` can update the same
user config later.

## Update

Run the pinned installer again with the new version:

```sh
curl -fsSL https://raw.githubusercontent.com/Ameyanagi/yuru/v0.1.4/install | sh -s -- --all --version v0.1.4
```

## Checksums

Release assets include `SHA256SUMS` on the GitHub release page. Download the asset and checksum file from the same release tag before verifying.

## Source Build Prerequisites

`cargo install yuru` builds from source. Japanese kanji readings use Lindera's
embedded IPADIC dictionary, and that build path currently compiles native crypto
code through `aws-lc-sys`. Install a C compiler first:

- macOS: Xcode Command Line Tools; the repo prefers `/usr/bin/clang` for Apple targets.
- Linux: `clang` or a distro build-essential package.
- Windows: Microsoft C++ Build Tools for the MSVC Rust toolchain.

Release binaries from GitHub do not require a local compiler.

The `image` feature is enabled by default and adds preview image decoding and
rendering through `ratatui-image`. Build without it when you want a smaller
source build:

```sh
cargo install yuru --no-default-features
```

## Uninstall

Remove the binary:

```sh
rm -f ~/.local/bin/yuru
```

Remove config if you do not want to keep it:

```sh
rm -rf ~/.config/yuru
```

Remove the block marked `yuru shell integration` from your shell profile, such as `~/.zshrc`, `~/.bashrc`, or `~/.config/fish/config.fish`.

On Windows, remove `%LOCALAPPDATA%\Yuru\bin\yuru.exe`, remove `%LOCALAPPDATA%\Yuru\bin` from the user PATH if desired, delete `%APPDATA%\yuru`, and remove the `yuru shell integration` block from the PowerShell profile.
