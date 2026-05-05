# fzf Compatibility

Yuru aims to feel familiar to fzf users, but it is not a full fzf clone. Unsupported parsed options warn by default, fail in strict mode, and are quiet in ignore mode.

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
| `--bind` | Partial | Supports `accept`, `abort`, and `clear-query` actions. |
| `--walker`, `--walker-root`, `--walker-skip` | Supported | Built-in walker respects `.gitignore`. |
| `--layout default|reverse|reverse-list`, `--reverse` | Supported | `default` places the prompt at the bottom and paints results bottom-up; `reverse` places prompt/results at the top; `reverse-list` places the prompt at the bottom with a top-down list. |
| `--preview` | Supported | Text preview pane; `{}` is replaced with the selected item. |
| `--color` | Partial | Supports `pointer`, `hl`, and `hl+` hex colors. Other entries are accepted and ignored. |
| `--preview-window`, `--border`, `--header-lines` | Accepted with warning | TUI styling compatibility is partial. |

`FZF_DEFAULT_OPTS` is loaded in safe mode by default. Safe mode keeps search/scripting options and drops UI-heavy or shell-execution options.

The shell bindings prefer `fd`, then `fdfind`, then `find` for path generation. They stream that output into Yuru and pass `--fzf-compat ignore`, so fzf-only UI options in `FZF_CTRL_T_OPTS` such as `--preview` do not produce warnings during key bindings.

fzf's default layout is bottom-up. Use `--layout=reverse` if you prefer Yuru's older top prompt, `--layout=default` for prompt-bottom/list-bottom-up, or `--layout=reverse-list` for prompt-bottom/list-top-down.

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
