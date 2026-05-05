#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
}

pub fn script(kind: ShellKind) -> &'static str {
    match kind {
        ShellKind::Bash => BASH,
        ShellKind::Zsh => ZSH,
        ShellKind::Fish => FISH,
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
