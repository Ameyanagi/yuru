# Troubleshooting

## Check Setup

Run:

```sh
yuru doctor
```

It reports the binary path, version, PATH visibility, config source, default language, fzf default option mode, locale, default command source, and shell integration marker.

## `fdfind: command not found`

Yuru's built-in walker does not require `fd` or `fdfind`. The error usually comes from `FZF_DEFAULT_COMMAND`, `YURU_DEFAULT_COMMAND`, or a shell integration command copied from an fzf setup.

Either install `fd`, change the command, or let Yuru use the built-in walker:

```sh
unset FZF_DEFAULT_COMMAND
unset YURU_DEFAULT_COMMAND
yuru --walker file,dir,follow,hidden
```

## Existing fzf options behave differently

Use:

```sh
yuru --fzf-compat strict
```

Then move unsupported UI-heavy options such as `--preview` into fzf-only config, or set:

```sh
yuru --load-fzf-default-opts never
```

## Japanese or Chinese does not match

Check the active language:

```sh
yuru doctor
```

Try an explicit mode:

```sh
yuru --lang ja --filter kamera
yuru --lang zh --filter bjdx
```

For Japanese kanji readings, `--ja-reading lindera` must be active.
