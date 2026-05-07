# fzf Compatibility

Yuru aims to feel familiar to fzf users, but it is not a full fzf clone. The CLI accepts fzf's current option surface so existing `FZF_DEFAULT_OPTS` and shell binding options do not fail at parse time. Unsupported `--bind` actions warn by default, fail in strict mode, and are quiet in ignore mode.

```sh
yuru --fzf-compat warn
yuru --fzf-compat strict
yuru --fzf-compat ignore
```

`YURU_FZF_COMPAT=strict|warn|ignore` sets the default.

| Area | Status | Notes |
| --- | --- | --- |
| `--query`, `--filter` | Supported | Non-interactive filtering and initial TUI query. |
| `--select-1`, `--exit-0`, `--print-query` | Supported | fzf-like scripting behavior. |
| `--read0`, `--print0` | Supported | Raw bytes are preserved for default output. |
| `--nth`, `--with-nth`, `--accept-nth`, `--delimiter` | Supported | Field transforms are intentionally smaller than fzf's full expression language. |
| `--scheme default|path|history` | Supported | Affects tiebreaks and ranking. |
| streaming input | Supported | Interactive mode can open while stdin or a default command is still producing candidates. |
| `--sync` | Supported | Waits for the input source before opening the interactive UI. |
| `--expect` | Supported | TUI output includes the accepted expected key. |
| `--bind` | Partial | Supports common navigation/editing actions, `accept`, `abort`, `clear-query`, mark toggles, and preview scroll actions. Shell actions such as `execute(...)` and `reload(...)` are still reported by compatibility mode. |
| `--header`, `--header-lines` | Supported | Explicit header text is shown in the TUI. Header lines are removed from the candidate set before search/output. |
| `--walker`, `--walker-root`, `--walker-skip` | Supported | Built-in walker respects `.gitignore`. |
| `--layout default|reverse|reverse-list`, `--reverse` | Supported | `default` places the prompt at the bottom and paints results bottom-up; `reverse` places prompt/results at the top; `reverse-list` places the prompt at the bottom with a top-down list. |
| `--preview` | Supported | Text preview pane; `{}` is replaced with the selected item. With the default `image` feature, preview commands that emit image bytes are rendered through `ratatui-image`. Scroll text with `shift-up`, `shift-down`, `shift-page-up`, and `shift-page-down`. |
| `--preview-auto` | Yuru extension | Built-in preview: render images internally, use `bat` for configured text extensions or ASCII text files when available, and fall back to `cat`-style plain text. |
| `--with-shell` | Supported for preview | Preview commands use the configured shell command. |
| `--multi[=MAX]`, `--multi MAX`, `-mMAX`, `--pointer`, `--marker`, `--ellipsis`, `--footer`, `--no-input` | Supported | Implemented in the current crossterm TUI. |
| `--color` | Partial | Supports `pointer`, `hl`, `hl+`, `fg+`, and `bg+` hex colors. Other entries are accepted and ignored. |
| Layout/style-only options such as `--preview-window`, `--border`, `--style`, labels, gutters, gaps, scrollbars, margins, padding | Accepted | Parsed for fzf config compatibility. Full visual parity with fzf is still evolving. |

Known gaps that matter for script migration:

| Area | Difference |
| --- | --- |
| Matcher algorithms | `--algo fzf-v1` uses Yuru's greedy scorer; `--algo fzf-v2` uses the nucleo-backed quality scorer. They are compatibility-inspired modes, not byte-for-byte fzf algorithm ports. |
| `--bind` shell actions | Navigation, editing, accept/abort, mark toggles, and preview scroll actions are implemented. Shell-execution actions such as `execute(...)`, `reload(...)`, and transform actions are still unsupported. |
| Field expressions | `--nth`, `--with-nth`, and `--accept-nth` cover common field selection and transforms, but not fzf's full expression language. |
| Layout/style parity | Many style options are accepted so existing option strings parse, but exact fzf visual parity is not guaranteed. |
| Non-interactive huge streams | `--filter` currently builds the candidate set before searching. Interactive mode streams candidates, but a line-by-line streaming top-k filter path is future work. |

`FZF_DEFAULT_OPTS` is loaded in safe mode by default. Safe mode keeps search/scripting options and drops UI-heavy or shell-execution options.

The shell bindings prefer `fd`, then `fdfind`, then `find` for path generation. They stream that output into Yuru and pass `--fzf-compat ignore`, so fzf-only UI options in `FZF_CTRL_T_OPTS` such as `--preview` do not produce warnings during key bindings.

fzf's default layout is bottom-up. Use `--layout=reverse` if you prefer Yuru's older top prompt, `--layout=default` for prompt-bottom/list-bottom-up, or `--layout=reverse-list` for prompt-bottom/list-top-down.

Preview scroll bind actions:

```sh
yuru --preview 'cat {}' --bind 'ctrl-k:preview-up,ctrl-j:preview-down'
yuru --preview 'cat {}' --bind 'ctrl-b:preview-page-up,ctrl-f:preview-page-down'
```

Image preview:

```sh
yuru --preview-auto
yuru --preview 'cat {}'
YURU_PREVIEW_IMAGE_PROTOCOL=sixel yuru --preview 'cat {}'
YURU_PREVIEW_IMAGE_PROTOCOL=kitty yuru --preview 'file {}'
```

`YURU_PREVIEW_IMAGE_PROTOCOL` accepts `halfblocks`, `sixel`, `kitty`, and `iterm2`.
`[preview] command = "auto"` enables built-in preview, and
`[preview] text_extensions = [...]` controls which extensions use the `bat` /
`cat` text path. Files outside that list also use the text path when their
contents look like ASCII text.
The TOML config option `[preview] image_protocol = "none"` is the default and
leaves this environment override plus automatic detection enabled. Set it to
`halfblocks`, `sixel`, `kitty`, or `iterm2` to force a protocol from config.
Without a forced protocol, Yuru uses safe environment hints and falls back to
halfblocks.
Ghostty is detected as Kitty protocol even inside tmux when `GHOSTTY_*` env vars
are present and tmux passthrough is enabled. Yuru also renders the selected file
directly when it is a raster image or SVG, so preview commands like `file {}` can
still show the image instead of plain metadata text. It does not call
`ratatui-image`'s stdio terminal query because Yuru reserves stdout for accepted
selections.

The image path is behind the `image` Cargo feature, which is enabled by default.
Use `cargo install yuru --no-default-features` to build a text-preview-only
binary.

```sh
yuru --load-fzf-default-opts never
yuru --load-fzf-default-opts safe
yuru --load-fzf-default-opts all
```

Precedence:

1. compiled defaults
2. safe subset of `FZF_DEFAULT_OPTS_FILE` / `FZF_DEFAULT_OPTS`
3. `~/.config/yuru/config.toml`
4. `YURU_DEFAULT_OPTS_FILE` / `YURU_DEFAULT_OPTS`
5. CLI arguments
