# Troubleshooting

## Check Setup

Run:

```sh
yuru doctor
```

It reports the binary path, version, PATH visibility, config source, default language, fzf default option mode, locale, default command source, and shell integration marker.

## `fdfind: command not found`

Yuru's shell integration now checks `fd`, then `fdfind`, then falls back to `find`. The built-in walker also does not require `fd` or `fdfind`. If you still see this error, it usually comes from `FZF_DEFAULT_COMMAND`, `YURU_DEFAULT_COMMAND`, or a custom `YURU_CTRL_T_COMMAND` / `FZF_CTRL_T_COMMAND`.

Either install `fd`, change the command, or let Yuru use the built-in walker:

```sh
unset FZF_DEFAULT_COMMAND
unset YURU_DEFAULT_COMMAND
yuru --walker file,dir,follow,hidden
```

## `CTRL-T` is slow in `$HOME`

The shell integration streams candidates from `fd` / `fdfind` / `find` into Yuru and does not follow symlinks by default. If traversal is still too broad, set a narrower command:

```sh
export YURU_CTRL_T_COMMAND='fd --hidden --exclude .git --exclude Library . ~/dev'
```

For exact fzf-style synchronous startup, use `--sync`; otherwise interactive mode opens while candidates are still being read.

## Existing fzf options behave differently

Use:

```sh
yuru --fzf-compat strict
```

Then move unsupported UI-heavy options into fzf-only config, or set:

```sh
yuru --load-fzf-default-opts never
```

## Source builds fail in `aws-lc-sys`

Yuru keeps Lindera enabled for Japanese kanji readings. Building from source can
therefore compile native code through `aws-lc-sys`. On macOS, install Xcode
Command Line Tools and use Apple clang. The repository config sets Apple target
`CC_*` variables to `/usr/bin/clang` unless you override them.

```sh
xcode-select --install
cargo install yuru
```

If your shell replaces `cc` with a non-Apple compiler, run:

```sh
CC=/usr/bin/clang cargo install yuru
```

## zsh says `read-only variable: status`

Install the current shell integration and reload zsh:

```sh
eval "$(yuru --zsh)"
```

Older Yuru zsh integration declared `status`, which is a zsh read-only special parameter.

## Japanese, Korean, or Chinese does not match

Check the active language:

```sh
yuru doctor
```

Try an explicit mode:

```sh
yuru --lang ja --filter kamera
yuru --lang ko --filter hangeul
yuru --lang zh --filter bjdx
```

For Japanese kanji readings, `--ja-reading lindera` must be active.
For Korean romanized, choseong, or keyboard search, keep
`--ko-romanization`, `--ko-initials`, or `--ko-keyboard` enabled.
