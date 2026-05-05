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

const BASH: &str = r#"# yomi shell integration for bash
# Install with: eval "$(yomi --bash)"

__yomi_join_bash__() {
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

__yomi_run_with_optional_command__() {
  local command_set="$1"
  local command_text="$2"
  shift 2
  if [ "$command_set" = 1 ]; then
    eval "$command_text" | "${YOMI_BIN:-yomi}" "$@"
  else
    "${YOMI_BIN:-yomi}" "$@"
  fi
}

__yomi_ctrl_t__() {
  local command_set=0 command_text selected insert opts
  if [ "${YOMI_CTRL_T_COMMAND+x}" ]; then
    command_set=1
    command_text=$YOMI_CTRL_T_COMMAND
  elif [ "${FZF_CTRL_T_COMMAND+x}" ]; then
    command_set=1
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=${YOMI_CTRL_T_OPTS:-${FZF_CTRL_T_OPTS:-}}
  selected=$(__yomi_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --walker file,dir,follow,hidden $opts)
  [ -n "$selected" ] || return
  insert=$(__yomi_join_bash__ "$selected")
  READLINE_LINE="${READLINE_LINE:0:READLINE_POINT}${insert}${READLINE_LINE:READLINE_POINT}"
  READLINE_POINT=$((READLINE_POINT + ${#insert}))
}

__yomi_ctrl_r__() {
  local selected opts
  opts=${YOMI_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}
  selected=$(HISTTIMEFORMAT= history | "${YOMI_BIN:-yomi}" --scheme history --tac --no-sort --no-multi $opts | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//')
  [ -n "$selected" ] || return
  READLINE_LINE=$selected
  READLINE_POINT=${#READLINE_LINE}
}

__yomi_alt_c__() {
  local command_set=0 command_text selected opts
  if [ "${YOMI_ALT_C_COMMAND+x}" ]; then
    command_set=1
    command_text=$YOMI_ALT_C_COMMAND
  elif [ "${FZF_ALT_C_COMMAND+x}" ]; then
    command_set=1
    command_text=$FZF_ALT_C_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=${YOMI_ALT_C_OPTS:-${FZF_ALT_C_OPTS:-}}
  selected=$(__yomi_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --walker dir,follow,hidden $opts)
  [ -n "$selected" ] || return
  builtin cd -- "$selected" || return
  READLINE_LINE=
  READLINE_POINT=0
}

__yomi_completion__() {
  local token prefix selected item opts
  token=${COMP_WORDS[COMP_CWORD]}
  if [[ "$token" != *"**" ]]; then
    return 1
  fi
  prefix=${token%\*\*}
  opts=${YOMI_COMPLETION_OPTS:-}
  selected=$("${YOMI_BIN:-yomi}" --scheme path -m --walker file,dir,follow,hidden --query "$prefix" $opts)
  [ -n "$selected" ] || return 1
  COMPREPLY=()
  while IFS= read -r item; do
    [ -n "$item" ] || continue
    COMPREPLY+=("$(printf '%q' "$item")")
  done <<< "$selected"
  return 0
}

bind -x '"\C-t": __yomi_ctrl_t__'
bind -x '"\C-r": __yomi_ctrl_r__'
bind -x '"\ec": __yomi_alt_c__'
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
complete -D -o default -o bashdefault -o nospace -F __yomi_completion__
"#;

const ZSH: &str = r#"# yomi shell integration for zsh
# Install with: source <(yomi --zsh)

__yomi_join_zsh__() {
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

__yomi_run_with_optional_command__() {
  emulate -L zsh
  local command_set="$1"
  local command_text="$2"
  shift 2
  if [[ "$command_set" == 1 ]]; then
    eval "$command_text" | "${YOMI_BIN:-yomi}" "$@"
  else
    "${YOMI_BIN:-yomi}" "$@"
  fi
}

__yomi_ctrl_t__() {
  emulate -L zsh
  local command_set=0 command_text selected insert opts
  if (( ${+YOMI_CTRL_T_COMMAND} )); then
    command_set=1
    command_text=$YOMI_CTRL_T_COMMAND
  elif (( ${+FZF_CTRL_T_COMMAND} )); then
    command_set=1
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=${YOMI_CTRL_T_OPTS:-${FZF_CTRL_T_OPTS:-}}
  selected=$(__yomi_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --walker file,dir,follow,hidden ${(z)opts})
  [[ -n "$selected" ]] || return
  insert=$(__yomi_join_zsh__ "$selected")
  LBUFFER="${LBUFFER}${insert}"
  zle reset-prompt
}

__yomi_ctrl_r__() {
  emulate -L zsh
  local selected opts
  opts=${YOMI_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}
  selected=$(fc -rl 1 | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//' | "${YOMI_BIN:-yomi}" --scheme history --tac --no-sort --no-multi ${(z)opts})
  [[ -n "$selected" ]] || return
  BUFFER=$selected
  CURSOR=${#BUFFER}
  zle reset-prompt
}

__yomi_alt_c__() {
  emulate -L zsh
  local command_set=0 command_text selected opts
  if (( ${+YOMI_ALT_C_COMMAND} )); then
    command_set=1
    command_text=$YOMI_ALT_C_COMMAND
  elif (( ${+FZF_ALT_C_COMMAND} )); then
    command_set=1
    command_text=$FZF_ALT_C_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=${YOMI_ALT_C_OPTS:-${FZF_ALT_C_OPTS:-}}
  selected=$(__yomi_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --walker dir,follow,hidden ${(z)opts})
  [[ -n "$selected" ]] || return
  builtin cd -- "$selected" || return
  zle reset-prompt
}

__yomi_completion__() {
  emulate -L zsh
  local token prefix selected insert opts keep
  token="${LBUFFER##*[[:space:]]}"
  if [[ "$token" != *"**" ]]; then
    zle expand-or-complete
    return
  fi
  prefix="${token%\*\*}"
  opts=${YOMI_COMPLETION_OPTS:-}
  selected=$("${YOMI_BIN:-yomi}" --scheme path -m --walker file,dir,follow,hidden --query "$prefix" ${(z)opts})
  [[ -n "$selected" ]] || return
  insert=$(__yomi_join_zsh__ "$selected")
  keep=$(( ${#LBUFFER} - ${#token} ))
  LBUFFER="${LBUFFER[1,$keep]}${insert}"
  zle reset-prompt
}

zle -N __yomi_ctrl_t__
zle -N __yomi_ctrl_r__
zle -N __yomi_alt_c__
zle -N __yomi_completion__
bindkey '^T' __yomi_ctrl_t__
bindkey '^R' __yomi_ctrl_r__
bindkey '^[c' __yomi_alt_c__
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
bindkey '^I' __yomi_completion__
"#;

const FISH: &str = r#"# yomi shell integration for fish
# Install with: yomi --fish | source

function __yomi_join_fish__
    string split \n -- $argv[1] | string match -v '' | string escape | string join ' '
end

function __yomi_ctrl_t__
    set -l yomi_bin (set -q YOMI_BIN; and echo $YOMI_BIN; or echo yomi)
    set -l command_set 0
    set -l command_text
    if set -q YOMI_CTRL_T_COMMAND
        set command_set 1
        set command_text $YOMI_CTRL_T_COMMAND
    else if set -q FZF_CTRL_T_COMMAND
        set command_set 1
        set command_text $FZF_CTRL_T_COMMAND
    end
    if test "$command_set" = 1; and test -z "$command_text"
        return
    end

    set -l opts $YOMI_CTRL_T_OPTS $FZF_CTRL_T_OPTS
    if test "$command_set" = 1
        set selected (eval $command_text | $yomi_bin --scheme path -m --walker file,dir,follow,hidden $opts)
    else
        set selected ($yomi_bin --scheme path -m --walker file,dir,follow,hidden $opts)
    end
    set -q selected[1]; or return
    commandline -i (__yomi_join_fish__ (string join \n -- $selected))
    commandline -f repaint
end

function __yomi_ctrl_r__
    set -l yomi_bin (set -q YOMI_BIN; and echo $YOMI_BIN; or echo yomi)
    set -l opts $YOMI_CTRL_R_OPTS $FZF_CTRL_R_OPTS
    set -l selected (history | $yomi_bin --scheme history --tac --no-sort --no-multi $opts)
    set -q selected[1]; or return
    commandline --replace "$selected"
    commandline --cursor (string length -- "$selected")
    commandline -f repaint
end

function __yomi_alt_c__
    set -l yomi_bin (set -q YOMI_BIN; and echo $YOMI_BIN; or echo yomi)
    set -l command_set 0
    set -l command_text
    if set -q YOMI_ALT_C_COMMAND
        set command_set 1
        set command_text $YOMI_ALT_C_COMMAND
    else if set -q FZF_ALT_C_COMMAND
        set command_set 1
        set command_text $FZF_ALT_C_COMMAND
    end
    if test "$command_set" = 1; and test -z "$command_text"
        return
    end

    set -l opts $YOMI_ALT_C_OPTS $FZF_ALT_C_OPTS
    if test "$command_set" = 1
        set selected (eval $command_text | $yomi_bin --scheme path --no-multi --walker dir,follow,hidden $opts)
    else
        set selected ($yomi_bin --scheme path --no-multi --walker dir,follow,hidden $opts)
    end
    set -q selected[1]; or return
    cd -- "$selected"; or return
    commandline --replace ''
    commandline -f repaint
end

function __yomi_completion__
    set -l yomi_bin (set -q YOMI_BIN; and echo $YOMI_BIN; or echo yomi)
    set -l left (commandline --cut-at-cursor)
    set -l token (string split -r -m1 ' ' -- $left)[-1]
    if not string match -q '*\*\*' -- $token
        commandline -f complete
        return
    end
    set -l prefix (string replace -r '\*\*$' '' -- $token)
    set -l selected ($yomi_bin --scheme path -m --walker file,dir,follow,hidden --query "$prefix" $YOMI_COMPLETION_OPTS)
    set -q selected[1]; or return
    set -l insert (__yomi_join_fish__ (string join \n -- $selected))
    commandline --current-token --replace "$insert"
    commandline -f repaint
end

bind \ct __yomi_ctrl_t__
bind \cr __yomi_ctrl_r__
bind \ec __yomi_alt_c__
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
bind \t __yomi_completion__
"#;

const POWERSHELL: &str = r#"# yomi shell integration for PowerShell
# Install with: yomi --powershell | Invoke-Expression

function Get-YomiCommand {
    if ($env:YOMI_BIN) { return $env:YOMI_BIN }
    return "yomi"
}

function Quote-YomiArgument {
    param([string]$Value)
    if ($Value -match '^[A-Za-z0-9_@%+=:,./\\-]+$') {
        return $Value
    }
    return "'" + ($Value -replace "'", "''") + "'"
}

function Join-YomiSelection {
    param([string[]]$Items)
    ($Items | Where-Object { $_ } | ForEach-Object { Quote-YomiArgument $_ }) -join " "
}

function Invoke-YomiWithOptionalCommand {
    param(
        [string]$CommandText,
        [string[]]$YomiArgs
    )
    $yomi = Get-YomiCommand
    if ($null -ne $CommandText) {
        if ($CommandText.Trim().Length -eq 0) { return @() }
        Invoke-Expression $CommandText | & $yomi @YomiArgs
    } else {
        & $yomi @YomiArgs
    }
}

function Invoke-YomiCtrlT {
    $commandText = $null
    if (Test-Path Env:YOMI_CTRL_T_COMMAND) {
        $commandText = $env:YOMI_CTRL_T_COMMAND
    } elseif (Test-Path Env:FZF_CTRL_T_COMMAND) {
        $commandText = $env:FZF_CTRL_T_COMMAND
    }
    $opts = @()
    if ($env:YOMI_CTRL_T_OPTS) { $opts += $env:YOMI_CTRL_T_OPTS -split '\s+' }
    elseif ($env:FZF_CTRL_T_OPTS) { $opts += $env:FZF_CTRL_T_OPTS -split '\s+' }
    $yomiArgs = @("--scheme", "path", "-m", "--walker", "file,dir,follow,hidden") + $opts
    $selected = @(Invoke-YomiWithOptionalCommand -CommandText $commandText -YomiArgs $yomiArgs)
    if ($selected.Count -eq 0) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert((Join-YomiSelection $selected))
}

function Invoke-YomiCtrlR {
    $yomi = Get-YomiCommand
    $opts = @()
    if ($env:YOMI_CTRL_R_OPTS) { $opts += $env:YOMI_CTRL_R_OPTS -split '\s+' }
    elseif ($env:FZF_CTRL_R_OPTS) { $opts += $env:FZF_CTRL_R_OPTS -split '\s+' }
    $selected = @(Get-History | ForEach-Object CommandLine | & $yomi --scheme history --tac --no-sort --no-multi @opts | Select-Object -First 1)
    if ($selected.Count -eq 0 -or [string]::IsNullOrEmpty($selected[0])) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($selected[0])
}

function Invoke-YomiAltC {
    $commandText = $null
    if (Test-Path Env:YOMI_ALT_C_COMMAND) {
        $commandText = $env:YOMI_ALT_C_COMMAND
    } elseif (Test-Path Env:FZF_ALT_C_COMMAND) {
        $commandText = $env:FZF_ALT_C_COMMAND
    }
    $opts = @()
    if ($env:YOMI_ALT_C_OPTS) { $opts += $env:YOMI_ALT_C_OPTS -split '\s+' }
    elseif ($env:FZF_ALT_C_OPTS) { $opts += $env:FZF_ALT_C_OPTS -split '\s+' }
    $yomiArgs = @("--scheme", "path", "--no-multi", "--walker", "dir,follow,hidden") + $opts
    $selected = @(Invoke-YomiWithOptionalCommand -CommandText $commandText -YomiArgs $yomiArgs | Select-Object -First 1)
    if ($selected.Count -eq 0 -or [string]::IsNullOrEmpty($selected[0])) { return }
    Set-Location -LiteralPath $selected[0]
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
}

function Invoke-YomiCompletion {
    param($key, $arg)
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $left = $line.Substring(0, $cursor)
    $match = [regex]::Match($left, '(\S*\*\*)$')
    if (-not $match.Success) {
        [Microsoft.PowerShell.PSConsoleReadLine]::Complete($key, $arg)
        return
    }
    $token = $match.Groups[1].Value
    $prefix = $token -replace '\*\*$', ''
    $yomi = Get-YomiCommand
    $opts = @()
    if ($env:YOMI_COMPLETION_OPTS) { $opts += $env:YOMI_COMPLETION_OPTS -split '\s+' }
    $selected = @(& $yomi --scheme path -m --walker file,dir,follow,hidden --query $prefix @opts)
    if ($selected.Count -eq 0) { return }
    $insert = Join-YomiSelection $selected
    $newLine = $line.Substring(0, $cursor - $token.Length) + $insert + $line.Substring($cursor)
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($newLine)
    [Microsoft.PowerShell.PSConsoleReadLine]::SetCursorPosition(($cursor - $token.Length) + $insert.Length)
}

if (Get-Module -ListAvailable -Name PSReadLine) {
    Import-Module PSReadLine -ErrorAction SilentlyContinue
}
if (Get-Command Set-PSReadLineKeyHandler -ErrorAction SilentlyContinue) {
    Set-PSReadLineKeyHandler -Key Ctrl+t -ScriptBlock { Invoke-YomiCtrlT }
    Set-PSReadLineKeyHandler -Key Ctrl+r -ScriptBlock { Invoke-YomiCtrlR }
    Set-PSReadLineKeyHandler -Key Alt+c -ScriptBlock { Invoke-YomiAltC }
    # fzf-style path completion trigger: COMMAND [FUZZY]**<Tab>
    Set-PSReadLineKeyHandler -Key Tab -ScriptBlock { param($key, $arg) Invoke-YomiCompletion $key $arg }
}
"#;
