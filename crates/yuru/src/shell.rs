#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

pub fn script(kind: ShellKind) -> &'static str {
    match kind {
        ShellKind::Bash => BASH,
        ShellKind::Zsh => ZSH,
        ShellKind::Fish => FISH,
        ShellKind::PowerShell => POWERSHELL,
    }
}

const BASH: &str = r#"# yuru shell integration for bash
# Install with: eval "$(yuru --bash)"

__yuru_join_bash__() {
  local item quoted out
  while IFS= read -r item; do
    [ -n "$item" ] || continue
    printf -v quoted '%q' "$item"
    if [ -n "$out" ]; then
      out="$out $quoted"
    else
      out="$quoted"
    fi
  done <<< "$1"
  printf '%s' "$out"
}

__yuru_run_with_optional_command__() {
  local command_set="$1" command_text="$2" tmp status
  shift 2
  if [ "$command_set" = 1 ]; then
    tmp="${TMPDIR:-/tmp}/yuru-command.$$"
    rm -f "$tmp"
    if eval "$command_text" >"$tmp" 2>/dev/null; then
      if [ -s "$tmp" ]; then
        "${YURU_BIN:-yuru}" "$@" --input "$tmp"
        status=$?
        rm -f "$tmp"
        return $status
      fi
    fi
    rm -f "$tmp"
  fi
  "${YURU_BIN:-yuru}" "$@"
}

__yuru_completion_trigger__() {
  printf '%s' "${YURU_COMPLETION_TRIGGER:-${FZF_COMPLETION_TRIGGER:-**}}"
}

__yuru_completion_opts__() {
  printf '%s' "${YURU_COMPLETION_OPTS:-${FZF_COMPLETION_OPTS:-}}"
}

__yuru_completion_command__() {
  local cmd="${COMP_WORDS[0]}"
  if [[ "$cmd" == \\* ]]; then
    cmd="${cmd:1}"
  fi
  printf '%s' "$cmd"
}

__yuru_completion_dirs_only__() {
  case "$(__yuru_completion_command__)" in
    cd|pushd|rmdir) return 0 ;;
    *) return 1 ;;
  esac
}

__yuru_ctrl_t__() {
  local command_set=0 command_text selected insert opts
  if [ "${YURU_CTRL_T_COMMAND+x}" ]; then
    command_set=1
    command_text=$YURU_CTRL_T_COMMAND
  elif [ "${FZF_CTRL_T_COMMAND+x}" ]; then
    command_set=1
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=${YURU_CTRL_T_OPTS:-${FZF_CTRL_T_OPTS:-}}
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --walker file,dir,follow,hidden $opts)
  [ -n "$selected" ] || return
  insert=$(__yuru_join_bash__ "$selected")
  [ -n "$insert" ] || return
  insert="$insert "
  READLINE_LINE="${READLINE_LINE:0:READLINE_POINT}${insert}${READLINE_LINE:READLINE_POINT}"
  READLINE_POINT=$((READLINE_POINT + ${#insert}))
}

__yuru_ctrl_r__() {
  local selected opts tmp status
  opts=${YURU_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}
  tmp="${TMPDIR:-/tmp}/yuru-history.$$"
  rm -f "$tmp"
  HISTTIMEFORMAT= history | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//' >"$tmp" || { rm -f "$tmp"; return; }
  selected=$("${YURU_BIN:-yuru}" --scheme history --tac --no-sort --no-multi --query "$READLINE_LINE" --input "$tmp" $opts)
  status=$?
  rm -f "$tmp"
  [ "$status" -eq 0 ] || return
  [ -n "$selected" ] || return
  READLINE_LINE=$selected
  READLINE_POINT=${#READLINE_LINE}
}

__yuru_alt_c__() {
  local command_set=0 command_text selected opts
  if [ "${YURU_ALT_C_COMMAND+x}" ]; then
    command_set=1
    command_text=$YURU_ALT_C_COMMAND
  elif [ "${FZF_ALT_C_COMMAND+x}" ]; then
    command_set=1
    command_text=$FZF_ALT_C_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=${YURU_ALT_C_OPTS:-${FZF_ALT_C_OPTS:-}}
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --walker dir,follow,hidden $opts)
  [ -n "$selected" ] || return
  builtin cd -- "$selected" || return
  READLINE_LINE=
  READLINE_POINT=0
}

__yuru_completion__() {
  local token trigger base dir root query selected insert opts walker multi
  token=${COMP_WORDS[COMP_CWORD]}
  trigger=$(__yuru_completion_trigger__)
  if [ -z "$trigger" ] || [[ "$token" != *"$trigger" ]]; then
    COMPREPLY=()
    return 0
  fi
  if [[ "$token" == *'$('* || "$token" == *':='* || "$token" == *'`'* ]]; then
    COMPREPLY=()
    return 0
  fi

  base=${token:0:${#token}-${#trigger}}
  eval "base=$base" 2>/dev/null || true
  dir=
  if [[ "$base" == */* ]]; then
    dir="$base"
    while [ -n "$dir" ] && [ ! -d "$dir" ]; do
      dir=$(dirname "$dir")
      [ "$dir" = "." ] && { dir=; break; }
    done
  fi

  root=${dir:-.}
  [ "$root" != "/" ] && root=${root%/}
  query=${base#"$root"}
  query=${query#/}
  opts=$(__yuru_completion_opts__)
  if __yuru_completion_dirs_only__; then
    walker=dir,follow,hidden
    multi=--no-multi
  else
    walker=file,dir,follow,hidden
    multi=-m
  fi

  selected=$("${YURU_BIN:-yuru}" --scheme path $multi --walker "$walker" --walker-root "$root" --query "$query" $opts)
  [ -n "$selected" ] || { COMPREPLY=("$token"); return 0; }
  insert=$(__yuru_join_bash__ "$selected")
  [ -n "$insert" ] || { COMPREPLY=("$token"); return 0; }
  COMPREPLY=("$insert")
  return 0
}

__yuru_setup_completion__() {
  local path_cmds dir_cmds cmd
  complete -D -o default -o bashdefault -o nospace -F __yuru_completion__ 2>/dev/null || true
  path_cmds=${YURU_COMPLETION_PATH_COMMANDS:-${FZF_COMPLETION_PATH_COMMANDS:-"awk bat cat code diff emacs file grep head less more nvim perl python ruby sed sort tail tee uniq vi view vim wc xdg-open chmod chown cp curl du find git gzip hg jar ln ls mv open rm rsync scp tar unzip zip"}}
  dir_cmds=${YURU_COMPLETION_DIR_COMMANDS:-${FZF_COMPLETION_DIR_COMMANDS:-"cd pushd rmdir"}}
  for cmd in $path_cmds $dir_cmds; do
    complete -o default -o bashdefault -o nospace -F __yuru_completion__ "$cmd" 2>/dev/null || true
  done
}

bind -x '"\C-t": __yuru_ctrl_t__'
bind -x '"\C-r": __yuru_ctrl_r__'
bind -x '"\ec": __yuru_alt_c__'
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
__yuru_setup_completion__
"#;

const ZSH: &str = r#"# yuru shell integration for zsh
# Install with: source <(yuru --zsh)

__yuru_join_zsh__() {
  emulate -L zsh
  local item out
  while IFS= read -r item; do
    [[ -n "$item" ]] || continue
    if [[ -n "$out" ]]; then
      out="$out ${(q)item}"
    else
      out="${(q)item}"
    fi
  done <<< "$1"
  print -r -- "$out"
}

__yuru_run_with_optional_command__() {
  emulate -L zsh
  local command_set="$1" command_text="$2" tmp status
  shift 2
  if [[ "$command_set" == 1 ]]; then
    tmp="${TMPDIR:-/tmp}/yuru-command.$$"
    rm -f "$tmp"
    if eval "$command_text" >"$tmp" 2>/dev/null; then
      if [[ -s "$tmp" ]]; then
        "${YURU_BIN:-yuru}" "$@" --input "$tmp"
        status=$?
        rm -f "$tmp"
        return $status
      fi
    fi
    rm -f "$tmp"
  fi
  "${YURU_BIN:-yuru}" "$@"
}

__yuru_completion_trigger__() {
  emulate -L zsh
  print -rn -- "${YURU_COMPLETION_TRIGGER:-${FZF_COMPLETION_TRIGGER:-**}}"
}

__yuru_completion_opts__() {
  emulate -L zsh
  print -rn -- "${YURU_COMPLETION_OPTS:-${FZF_COMPLETION_OPTS:-}}"
}

__yuru_completion_command__() {
  emulate -L zsh
  local -a words
  words=(${(z)LBUFFER})
  print -rn -- "${words[1]:-}"
}

__yuru_completion_dirs_only__() {
  emulate -L zsh
  case "$(__yuru_completion_command__)" in
    cd|pushd|rmdir) return 0 ;;
    *) return 1 ;;
  esac
}

__yuru_fallback_completion__() {
  emulate -L zsh
  zle ${__yuru_default_completion_widget:-expand-or-complete}
}

__yuru_ctrl_t__() {
  emulate -L zsh
  local command_set=0 command_text selected insert opts
  if (( ${+YURU_CTRL_T_COMMAND} )); then
    command_set=1
    command_text=$YURU_CTRL_T_COMMAND
  elif (( ${+FZF_CTRL_T_COMMAND} )); then
    command_set=1
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=${YURU_CTRL_T_OPTS:-${FZF_CTRL_T_OPTS:-}}
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --walker file,dir,follow,hidden ${(z)opts})
  [[ -n "$selected" ]] || return
  insert=$(__yuru_join_zsh__ "$selected")
  [[ -n "$insert" ]] || return
  LBUFFER="${LBUFFER}${insert} "
  zle reset-prompt
}

__yuru_ctrl_r__() {
  emulate -L zsh
  local selected opts tmp status
  opts=${YURU_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}
  tmp="${TMPDIR:-/tmp}/yuru-history.$$"
  rm -f "$tmp"
  fc -rl 1 | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//' >"$tmp" || { rm -f "$tmp"; return }
  selected=$("${YURU_BIN:-yuru}" --scheme history --tac --no-sort --no-multi --query "$LBUFFER" --input "$tmp" ${(z)opts})
  status=$?
  rm -f "$tmp"
  (( status == 0 )) || return
  [[ -n "$selected" ]] || return
  BUFFER=$selected
  CURSOR=${#BUFFER}
  zle reset-prompt
}

__yuru_alt_c__() {
  emulate -L zsh
  local command_set=0 command_text selected opts
  if (( ${+YURU_ALT_C_COMMAND} )); then
    command_set=1
    command_text=$YURU_ALT_C_COMMAND
  elif (( ${+FZF_ALT_C_COMMAND} )); then
    command_set=1
    command_text=$FZF_ALT_C_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=${YURU_ALT_C_OPTS:-${FZF_ALT_C_OPTS:-}}
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --walker dir,follow,hidden ${(z)opts})
  [[ -n "$selected" ]] || return
  builtin cd -- "$selected" || return
  zle reset-prompt
}

__yuru_completion__() {
  emulate -L zsh
  local token trigger base dir root query selected insert opts keep walker multi
  token="${LBUFFER##*[[:space:]]}"
  trigger=$(__yuru_completion_trigger__)
  if [[ -z "$trigger" || "$token" != *"$trigger" ]]; then
    __yuru_fallback_completion__
    return
  fi
  if [[ "$token" = *'$('* || "$token" = *'<('* || "$token" = *'>('* || "$token" = *':='* || "$token" = *'`'* ]]; then
    __yuru_fallback_completion__
    return
  fi

  base="${token[1,$(( ${#token} - ${#trigger} ))]}"
  eval "base=$base" 2>/dev/null || true
  dir=
  if [[ "$base" == */* ]]; then
    dir="$base"
    while [[ -n "$dir" && ! -d "$dir" ]]; do
      dir=$(dirname "$dir")
      [[ "$dir" == "." ]] && { dir=; break; }
    done
  fi

  root=${dir:-.}
  [[ "$root" != "/" ]] && root=${root%/}
  query=${base#"$root"}
  query=${query#/}
  opts=$(__yuru_completion_opts__)
  if __yuru_completion_dirs_only__; then
    walker=dir,follow,hidden
    multi=--no-multi
  else
    walker=file,dir,follow,hidden
    multi=-m
  fi

  selected=$("${YURU_BIN:-yuru}" --scheme path $multi --walker "$walker" --walker-root "$root" --query "$query" ${(z)opts})
  [[ -n "$selected" ]] || return
  insert=$(__yuru_join_zsh__ "$selected")
  [[ -n "$insert" ]] || return
  keep=$(( ${#LBUFFER} - ${#token} ))
  if (( keep > 0 )); then
    LBUFFER="${LBUFFER[1,$keep]}${insert} "
  else
    LBUFFER="${insert} "
  fi
  zle reset-prompt
}

if [[ -z ${__yuru_default_completion_widget-} ]]; then
  __yuru_tab_binding=$(bindkey '^I' 2>/dev/null)
  if [[ "$__yuru_tab_binding" != *undefined-key* ]]; then
    __yuru_default_completion_widget=${__yuru_tab_binding[(s: :w)2]}
  fi
  unset __yuru_tab_binding
fi

zle -N __yuru_ctrl_t__
zle -N __yuru_ctrl_r__
zle -N __yuru_alt_c__
zle -N __yuru_completion__
bindkey -M emacs '^T' __yuru_ctrl_t__
bindkey -M emacs '^R' __yuru_ctrl_r__
bindkey -M emacs '^[c' __yuru_alt_c__
bindkey -M viins '^T' __yuru_ctrl_t__
bindkey -M viins '^R' __yuru_ctrl_r__
bindkey -M viins '^[c' __yuru_alt_c__
bindkey -M vicmd '^T' __yuru_ctrl_t__
bindkey -M vicmd '^R' __yuru_ctrl_r__
bindkey -M vicmd '^[c' __yuru_alt_c__
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
bindkey -M emacs '^I' __yuru_completion__
bindkey -M viins '^I' __yuru_completion__
"#;

const FISH: &str = r#"# yuru shell integration for fish
# Install with: yuru --fish | source

function __yuru_join_fish__
    string split \n -- $argv[1] | string match -v '' | string escape | string join ' '
end

function __yuru_completion_trigger__
    if set -q YURU_COMPLETION_TRIGGER
        printf '%s' $YURU_COMPLETION_TRIGGER
    else if set -q FZF_COMPLETION_TRIGGER
        printf '%s' $FZF_COMPLETION_TRIGGER
    else
        printf '**'
    end
end

function __yuru_completion_opts__
    if set -q YURU_COMPLETION_OPTS
        string split ' ' -- $YURU_COMPLETION_OPTS
    else if set -q FZF_COMPLETION_OPTS
        string split ' ' -- $FZF_COMPLETION_OPTS
    end
end

function __yuru_completion_dirs_only__
    set -l command (commandline -opc)[1]
    switch $command
        case cd pushd rmdir
            return 0
        case '*'
            return 1
    end
end

function __yuru_run_with_optional_command__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l command_set $argv[1]
    set -l command_text $argv[2]
    set -e argv[1]
    set -e argv[1]

    if test "$command_set" = 1
        set -l tmpdir /tmp
        if set -q TMPDIR; and test -n "$TMPDIR"
            set tmpdir $TMPDIR
        end
        set -l tmp (mktemp "$tmpdir/yuru-command.XXXXXX")
        if eval $command_text >$tmp 2>/dev/null; and test -s "$tmp"
            $yuru_bin $argv --input "$tmp"
            set -l status_code $status
            rm -f "$tmp"
            return $status_code
        end
        rm -f "$tmp"
    end

    $yuru_bin $argv
end

function __yuru_ctrl_t__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l command_set 0
    set -l command_text
    if set -q YURU_CTRL_T_COMMAND
        set command_set 1
        set command_text $YURU_CTRL_T_COMMAND
    else if set -q FZF_CTRL_T_COMMAND
        set command_set 1
        set command_text $FZF_CTRL_T_COMMAND
    end
    if test "$command_set" = 1; and test -z "$command_text"
        return
    end

    set -l opts
    if set -q YURU_CTRL_T_OPTS
        set opts (string split ' ' -- $YURU_CTRL_T_OPTS)
    else if set -q FZF_CTRL_T_OPTS
        set opts (string split ' ' -- $FZF_CTRL_T_OPTS)
    end
    set selected (__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --walker file,dir,follow,hidden $opts)
    set -q selected[1]; or return
    set -l insert (__yuru_join_fish__ (string join \n -- $selected))
    commandline -i "$insert "
    commandline -f repaint
end

function __yuru_ctrl_r__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l opts
    if set -q YURU_CTRL_R_OPTS
        set opts (string split ' ' -- $YURU_CTRL_R_OPTS)
    else if set -q FZF_CTRL_R_OPTS
        set opts (string split ' ' -- $FZF_CTRL_R_OPTS)
    end
    set -l tmpdir /tmp
    if set -q TMPDIR; and test -n "$TMPDIR"
        set tmpdir $TMPDIR
    end
    set -l tmp (mktemp "$tmpdir/yuru-history.XXXXXX")
    history >$tmp
    set -l selected ($yuru_bin --scheme history --tac --no-sort --no-multi --query (commandline) --input "$tmp" $opts)
    set -l status_code $status
    rm -f "$tmp"
    test $status_code -eq 0; or return
    set -q selected[1]; or return
    commandline --replace "$selected"
    commandline --cursor (string length -- "$selected")
    commandline -f repaint
end

function __yuru_alt_c__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l command_set 0
    set -l command_text
    if set -q YURU_ALT_C_COMMAND
        set command_set 1
        set command_text $YURU_ALT_C_COMMAND
    else if set -q FZF_ALT_C_COMMAND
        set command_set 1
        set command_text $FZF_ALT_C_COMMAND
    end
    if test "$command_set" = 1; and test -z "$command_text"
        return
    end

    set -l opts
    if set -q YURU_ALT_C_OPTS
        set opts (string split ' ' -- $YURU_ALT_C_OPTS)
    else if set -q FZF_ALT_C_OPTS
        set opts (string split ' ' -- $FZF_ALT_C_OPTS)
    end
    set selected (__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --walker dir,follow,hidden $opts)
    set -q selected[1]; or return
    cd -- "$selected"; or return
    commandline --replace ''
    commandline -f repaint
end

function __yuru_completion__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l token (commandline --current-token)
    set -l trigger (__yuru_completion_trigger__)
    if test -z "$trigger"; or not string match -q "*$trigger" -- $token
        commandline -f complete
        return
    end
    set -l base_len (math (string length -- "$token") - (string length -- "$trigger"))
    set -l base
    if test "$base_len" -gt 0
        set base (string sub -l $base_len -- "$token")
    else
        set base ''
    end

    set -l root .
    set -l query "$base"
    if string match -q '*/*' -- "$base"
        set -l dir "$base"
        while test -n "$dir"; and not test -d "$dir"
            set dir (dirname "$dir")
            if test "$dir" = "."
                set dir ''
                break
            end
        end
        if test -n "$dir"
            set root (string replace -r '/$' '' -- "$dir")
            test -n "$root"; or set root /
            set query (string replace -r '^\Q'"$root"'\E/?' '' -- "$base")
        end
    end

    set -l opts (__yuru_completion_opts__)
    set -l selected
    if __yuru_completion_dirs_only__
        set selected ($yuru_bin --scheme path --no-multi --walker dir,follow,hidden --walker-root "$root" --query "$query" $opts)
    else
        set selected ($yuru_bin --scheme path -m --walker file,dir,follow,hidden --walker-root "$root" --query "$query" $opts)
    end
    set -q selected[1]; or return
    set -l insert (__yuru_join_fish__ (string join \n -- $selected))
    commandline --current-token --replace "$insert "
    commandline -f repaint
end

bind \ct __yuru_ctrl_t__
bind \cr __yuru_ctrl_r__
bind \ec __yuru_alt_c__
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
bind \t __yuru_completion__
"#;

const POWERSHELL: &str = r#"# yuru shell integration for PowerShell
# Install with: yuru --powershell | Invoke-Expression

function Get-YuruCommand {
    if ($env:YURU_BIN) { return $env:YURU_BIN }
    return "yuru"
}

function Quote-YuruArgument {
    param([string]$Value)
    if ($Value -match '^[A-Za-z0-9_@%+=:,./\\-]+$') {
        return $Value
    }
    return "'" + ($Value -replace "'", "''") + "'"
}

function Join-YuruSelection {
    param([string[]]$Items)
    ($Items | Where-Object { $_ } | ForEach-Object { Quote-YuruArgument $_ }) -join " "
}

function Get-YuruCompletionTrigger {
    if ($env:YURU_COMPLETION_TRIGGER) { return $env:YURU_COMPLETION_TRIGGER }
    if ($env:FZF_COMPLETION_TRIGGER) { return $env:FZF_COMPLETION_TRIGGER }
    return "**"
}

function Split-YuruOptions {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return @() }
    return @($Value -split '\s+' | Where-Object { $_ })
}

function Get-YuruCompletionOptions {
    if ($env:YURU_COMPLETION_OPTS) {
        Split-YuruOptions $env:YURU_COMPLETION_OPTS
        return
    }
    if ($env:FZF_COMPLETION_OPTS) {
        Split-YuruOptions $env:FZF_COMPLETION_OPTS
        return
    }
    return @()
}

function Test-YuruDirectoryCompletion {
    param([string]$Line)
    $trimmed = $Line.TrimStart()
    return $trimmed -match '^(cd|pushd|rmdir)(\s|$)'
}

function Get-YuruHistoryLines {
    $items = New-Object System.Collections.Generic.List[string]
    try {
        if (Get-Command Get-PSReadLineOption -ErrorAction SilentlyContinue) {
            $historyPath = (Get-PSReadLineOption).HistorySavePath
            if ($historyPath -and (Test-Path -LiteralPath $historyPath)) {
                Get-Content -LiteralPath $historyPath -ErrorAction SilentlyContinue | ForEach-Object {
                    if ($_ -and $_.Trim().Length -gt 0) { $items.Add($_) }
                }
            }
        }
    } catch {}
    Get-History | ForEach-Object CommandLine | ForEach-Object {
        if ($_ -and $_.Trim().Length -gt 0) { $items.Add($_) }
    }
    $items | Select-Object -Unique
}

function Invoke-YuruWithItems {
    param(
        [string[]]$Items,
        [string[]]$YuruArgs
    )
    $itemsArray = @($Items | Where-Object { $_ })
    if ($itemsArray.Count -eq 0) { return @() }

    $yuru = Get-YuruCommand
    $tmp = [System.IO.Path]::GetTempFileName()
    try {
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllLines($tmp, [string[]]$itemsArray, $utf8NoBom)
        & $yuru @($YuruArgs + @("--input", $tmp))
    } finally {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-YuruWithOptionalCommand {
    param(
        [string]$CommandText,
        [string[]]$YuruArgs
    )
    $yuru = Get-YuruCommand
    if ($null -ne $CommandText) {
        if ($CommandText.Trim().Length -eq 0) { return @() }
        $items = @()
        try {
            $items = @(Invoke-Expression $CommandText 2>$null)
        } catch {
            $items = @()
        }
        if ($items.Count -gt 0) {
            return Invoke-YuruWithItems -Items $items -YuruArgs $YuruArgs
        }
    }
    & $yuru @YuruArgs
}

function Invoke-YuruCtrlT {
    $commandText = $null
    if (Test-Path Env:YURU_CTRL_T_COMMAND) {
        $commandText = $env:YURU_CTRL_T_COMMAND
    } elseif (Test-Path Env:FZF_CTRL_T_COMMAND) {
        $commandText = $env:FZF_CTRL_T_COMMAND
    }
    $opts = @()
    if ($env:YURU_CTRL_T_OPTS) { $opts += @(Split-YuruOptions $env:YURU_CTRL_T_OPTS) }
    elseif ($env:FZF_CTRL_T_OPTS) { $opts += @(Split-YuruOptions $env:FZF_CTRL_T_OPTS) }
    $yuruArgs = @("--scheme", "path", "-m", "--walker", "file,dir,follow,hidden") + $opts
    $selected = @(Invoke-YuruWithOptionalCommand -CommandText $commandText -YuruArgs $yuruArgs)
    if ($selected.Count -eq 0) { return }
    $insert = Join-YuruSelection $selected
    if ([string]::IsNullOrEmpty($insert)) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($insert + " ")
}

function Invoke-YuruCtrlR {
    $yuru = Get-YuruCommand
    $opts = @()
    if ($env:YURU_CTRL_R_OPTS) { $opts += @(Split-YuruOptions $env:YURU_CTRL_R_OPTS) }
    elseif ($env:FZF_CTRL_R_OPTS) { $opts += @(Split-YuruOptions $env:FZF_CTRL_R_OPTS) }
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $yuruArgs = @("--scheme", "history", "--tac", "--no-sort", "--no-multi", "--query", $line) + $opts
    $selected = @(Invoke-YuruWithItems -Items @(Get-YuruHistoryLines) -YuruArgs $yuruArgs | Select-Object -First 1)
    if ($selected.Count -eq 0 -or [string]::IsNullOrEmpty($selected[0])) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($selected[0])
}

function Invoke-YuruAltC {
    $commandText = $null
    if (Test-Path Env:YURU_ALT_C_COMMAND) {
        $commandText = $env:YURU_ALT_C_COMMAND
    } elseif (Test-Path Env:FZF_ALT_C_COMMAND) {
        $commandText = $env:FZF_ALT_C_COMMAND
    }
    $opts = @()
    if ($env:YURU_ALT_C_OPTS) { $opts += @(Split-YuruOptions $env:YURU_ALT_C_OPTS) }
    elseif ($env:FZF_ALT_C_OPTS) { $opts += @(Split-YuruOptions $env:FZF_ALT_C_OPTS) }
    $yuruArgs = @("--scheme", "path", "--no-multi", "--walker", "dir,follow,hidden") + $opts
    $selected = @(Invoke-YuruWithOptionalCommand -CommandText $commandText -YuruArgs $yuruArgs | Select-Object -First 1)
    if ($selected.Count -eq 0 -or [string]::IsNullOrEmpty($selected[0])) { return }
    Set-Location -LiteralPath $selected[0]
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
}

function Invoke-YuruCompletion {
    param($key, $arg)
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $left = $line.Substring(0, $cursor)
    $trigger = Get-YuruCompletionTrigger
    if ([string]::IsNullOrEmpty($trigger)) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Complete($key, $arg)
        return
    }
    $escapedTrigger = [regex]::Escape($trigger)
    $match = [regex]::Match($left, "(\S*$escapedTrigger)$")
    if (-not $match.Success) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Complete($key, $arg)
        return
    }
    $token = $match.Groups[1].Value
    $base = $token.Substring(0, $token.Length - $trigger.Length)
    $root = "."
    $query = $base
    if ($base -like "*/*" -or $base -like "*\*") {
        $dir = $base
        while ($dir -and -not (Test-Path -LiteralPath $dir -PathType Container)) {
            $parent = Split-Path -Parent $dir
            if ([string]::IsNullOrEmpty($parent) -or $parent -eq $dir) {
                $dir = $null
                break
            }
            $dir = $parent
        }
        if ($dir) {
            $root = $dir
            $query = $base.Substring($dir.Length).TrimStart([char[]]@([char]'/', [char]'\'))
        }
    }
    $yuru = Get-YuruCommand
    $opts = @(Get-YuruCompletionOptions)
    if (Test-YuruDirectoryCompletion $left) {
        $selected = @(& $yuru --scheme path --no-multi --walker dir,follow,hidden --walker-root $root --query $query @opts)
    } else {
        $selected = @(& $yuru --scheme path -m --walker file,dir,follow,hidden --walker-root $root --query $query @opts)
    }
    if ($selected.Count -eq 0) { return }
    $insert = Join-YuruSelection $selected
    if ([string]::IsNullOrEmpty($insert)) { return }
    $insert = $insert + " "
    $newLine = $line.Substring(0, $cursor - $token.Length) + $insert + $line.Substring($cursor)
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($newLine)
    [Microsoft.PowerShell.PSConsoleReadLine]::SetCursorPosition(($cursor - $token.Length) + $insert.Length)
}

if (Get-Module -ListAvailable -Name PSReadLine) {
    Import-Module PSReadLine -ErrorAction SilentlyContinue
}
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Key Ctrl+t -ScriptBlock { Invoke-YuruCtrlT }
    Set-PSReadLineKeyHandler -Key Ctrl+r -ScriptBlock { Invoke-YuruCtrlR }
    Set-PSReadLineKeyHandler -Key Alt+c -ScriptBlock { Invoke-YuruAltC }
    # fzf-style path completion trigger: COMMAND [FUZZY]**<Tab>
    Set-PSReadLineKeyHandler -Key Tab -ScriptBlock { param($key, $arg) Invoke-YuruCompletion $key $arg }
}
"#;
