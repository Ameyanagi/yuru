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
| `--expect` | Supported | TUI output includes the accepted expected key. |
| `--bind` | Partial | Supports `accept`, `abort`, and `clear-query` actions. |
| `--walker`, `--walker-root`, `--walker-skip` | Supported | Built-in walker respects `.gitignore`. |
| `--preview`, `--preview-window` | Accepted with warning | Preview UI is planned. |
| `--layout`, `--border`, `--color`, `--header-lines` | Accepted with warning | TUI styling compatibility is partial. |

`FZF_DEFAULT_OPTS` is loaded in safe mode by default. Safe mode keeps search/scripting options and drops UI-heavy or shell-execution options.

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
