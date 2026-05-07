# Configuration

Yuru reads `~/.config/yuru/config.toml` on Unix and `%APPDATA%\yuru\config.toml` on Windows. Set `YURU_CONFIG_FILE` to use a different file.

```toml
[defaults]
lang = "auto"          # plain | ja | ko | zh | auto
scheme = "path"        # default | path | history
case = "smart"         # smart | ignore | respect
limit = 200
load_fzf_defaults = "safe"
fzf_compat = "warn"

[preview]
command = "auto"        # auto | none | shell command
text_extensions = [
  "txt", "md", "markdown", "toml", "json", "yaml", "yml", "csv", "tsv",
  "log", "rs", "py", "js", "ts", "tsx", "sh", "ps1", "sql", "html", "css",
]
image_protocol = "none" # none | halfblocks | sixel | kitty | iterm2

[matching]
algo = "greedy"        # greedy | fzf-v1 | fzf-v2 | nucleo
max_query_variants = 8
max_search_keys_per_candidate = 8
max_total_key_bytes_per_candidate = 1024

[ja]
reading = "lindera"    # none | lindera

[ko]
romanization = true
initials = true
keyboard = true

[zh]
pinyin = true
initials = true
polyphone = "common"   # none | common

[shell]
bindings = "all"       # all | none | ctrl-t,ctrl-r,alt-c,completion
path_backend = "auto"  # auto | fd | fdfind | find
ctrl_t_command = "__yuru_compgen_path__ ."
ctrl_t_opts = "--preview-auto"
alt_c_command = "__yuru_compgen_dir__ ."
alt_c_opts = "--preview-auto"
```

`lang = "auto"` chooses one active backend per run. It does not build every language key for every candidate.

`[matching].algo` selects Yuru matcher backends, not byte-for-byte fzf
algorithm implementations. `greedy` and `fzf-v1` use Yuru's greedy scorer.
`fzf-v2` and `nucleo` use the nucleo-backed quality scorer. Normal nucleo
searches parallelize on large inputs, but extended-syntax nucleo searches still
spend more work per candidate and can be slower.

`[ko]` controls Korean Hangul keys. `romanization` enables deterministic
Romanization-style keys such as `hangeul`, `initials` enables choseong keys such
as `ㅎㄱ`, and `keyboard` enables Korean 2-set keyboard-layout keys such as
`gksrmf`.

`[zh].polyphone = "none"` keeps the primary pinyin reading for each character.
`"common"` also adds a small, capped set of heteronym alternatives. The older
`"phrase"` value is still accepted for compatibility, but currently warns and
uses `"common"` behavior. `[zh].script` is reserved and currently has no effect;
it is intentionally omitted from the default config example.

`preview.image_protocol = "none"` leaves image previews on automatic terminal
detection and still allows `YURU_PREVIEW_IMAGE_PROTOCOL` to override it. Choose
`kitty`, `sixel`, `iterm2`, or `halfblocks` to force a protocol from config.

`preview.command = "auto"` enables Yuru's built-in preview. It renders image
paths internally, uses `bat` for configured text extensions when available, and
falls back to `cat`-style plain text output. Set it to `none` to disable preview or to a
shell command to use traditional `--preview` behavior. `preview.text_extensions`
defines which extensions always use the text path. Files outside that list also
use the text path when their contents look like ASCII text.

`shell.path_backend = "auto"` tries `fd`, then `fdfind`, then `find` for shell
path search. Set it to `fd`, `fdfind`, or `find` to prefer a specific backend.
Custom `ctrl_t_command` and `alt_c_command` values still take precedence.

Legacy shell-word config files named `config` are still read, but Yuru warns and prefers `config.toml`.

Use `yuru configure` to reconfigure these values interactively. It reads the
current config first and uses those values as prompt defaults.

Use `yuru doctor` to see which config source was detected.
