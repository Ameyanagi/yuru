# Changelog

All notable user-facing changes are tracked here.

## 0.1.5

- Added Korean Hangul matching support, including romanization, choseong initials, and Korean 2-set keyboard-layout keys.
- Added built-in preview configuration with `bat` / `cat` text fallback, ASCII text sniffing, internal image rendering, and selectable image protocols.
- Added Chinese `zh.polyphone = "common"` heteronym expansion with capped alternate pinyin keys; reserved `phrase` and `script` options now warn or stay hidden.
- Added README/demo and internals documentation covering multilingual indexing/search complexity, fzf comparison, preview internals, and agentic coding disclosure.
- Hardened release publishing by including `yuru-ko` in the crates.io publish order and validating omitted workspace dependencies before tag releases.
- Clarified matcher algorithm names so `fzf-v1` / `fzf-v2` are documented as compatibility-inspired modes, not byte-for-byte fzf implementations.

## 0.1.4

- Added streaming interactive input for stdin and default source commands, with `--sync` for fzf-style synchronous startup.
- Updated shell integrations to stream `fd` / `fdfind` / `find` output into Yuru instead of preloading command output into temp files.
- Made `CTRL-T`, `ALT-C`, and `**<TAB>` avoid following symlinks by default and skip macOS `Library` in generated shell candidates.
- Suppressed fzf-only UI option warnings such as `--preview` inside shell key bindings.
- Hardened the built-in walker to skip filesystem loop errors when following symlinks.
- Added fzf-style bottom prompt layout support with bottom-up result painting for `--layout=default`, plus `--layout=reverse-list` and `--reverse`.
- Added text preview support for `--preview`, including stderr/failure text, and partial `--color` support for `pointer`, `hl`, and `hl+`.
- Fixed zsh shell integration by avoiding the read-only `status` parameter.
- Improved Japanese numeric date matching so `2025年8月` can match `20258gatsu`, `2025nen8gatsu`, `8gatsu`, `はち`, and literal `2025`.

## 0.1.3

- Added `yuru doctor` for local setup diagnostics.
- Added `--explain` and `--debug-match` for inspecting winning match keys and source spans.
- Added fzf compatibility, configuration, language matching, troubleshooting, install, security, and contributor docs.
- Added README badges, localized README updates, demo assets, and release-pinned install examples.
- Added golden ranking tests, matcher property tests, MSRV checks, and supply-chain audit policy.
- Updated benchmark reporting, including 1M and kanji-heavy benchmark numbers.
- Improved release notes to point at tag-pinned installer scripts.

## 0.1.2

- Renamed the project from Yomi to Yuru.
- Published release assets for Linux, macOS Intel, macOS Apple Silicon, and Windows.
- Published crates.io packages and added release installer smoke tests.
- Improved shell integration for `CTRL-T`, `CTRL-R`, `ALT-C`, and `**<TAB>`.
- Added Lindera-backed Japanese readings, Chinese pinyin source maps, and CJK highlight fixes.
- Added fzf compatibility controls, config precedence, and release-only publishing workflow.

## 0.1.1

- First Yuru release after the rename.
- Added user-space installers and shell configuration support.
- Added localized README files for Japanese, Chinese, and Korean.

## 0.1.0

- Historical Yomi release.
- Implemented the initial fuzzy finder, phonetic matching backends, TUI, shell integration, and benchmarks.
