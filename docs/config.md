# Configuration

Yuru reads `~/.config/yuru/config.toml` on Unix and `%APPDATA%\yuru\config.toml` on Windows. Set `YURU_CONFIG_FILE` to use a different file.

```toml
[defaults]
lang = "auto"          # plain | ja | zh | auto
scheme = "path"        # default | path | history
case = "smart"         # smart | ignore | respect
limit = 200
load_fzf_defaults = "safe"
fzf_compat = "warn"

[matching]
algo = "greedy"        # greedy | fzf-v1 | fzf-v2 | nucleo
max_query_variants = 8
max_search_keys_per_candidate = 8
max_total_key_bytes_per_candidate = 1024

[ja]
reading = "lindera"    # none | lindera

[zh]
pinyin = true
initials = true
polyphone = "common"   # none | common | phrase
script = "auto"        # auto | hans | hant

[shell]
bindings = "all"       # all | none | ctrl-t,ctrl-r,alt-c,completion
ctrl_t_command = "__yuru_compgen_path__ ."
ctrl_t_opts = "--preview 'file {}'"
alt_c_command = "__yuru_compgen_dir__ ."
alt_c_opts = "--preview 'ls -la {} 2>/dev/null | head -100'"
```

`lang = "auto"` chooses one active backend per run. It does not build every language key for every candidate.

Legacy shell-word config files named `config` are still read, but Yuru warns and prefers `config.toml`.

Use `yuru configure` to reconfigure these values interactively. It reads the
current config first and uses those values as prompt defaults.

Use `yuru doctor` to see which config source was detected.
