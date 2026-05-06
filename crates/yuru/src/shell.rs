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

__yuru_binding_enabled__() {
  local name="$1" bindings=",${YURU_SHELL_BINDINGS:-all},"
  bindings="${bindings// /,}"
  case "$bindings" in
    *,all,*) return 0 ;;
    *,none,*) return 1 ;;
    *,"$name",*) return 0 ;;
  esac
  if [ "$name" = completion ]; then
    case "$bindings" in
      *,tab,* | *,path-completion,*) return 0 ;;
    esac
  fi
  return 1
}

__yuru_ctrl_t_opts__() {
  if [ "${YURU_CTRL_T_OPTS+x}" ]; then
    printf '%s' "$YURU_CTRL_T_OPTS"
  elif [ "${FZF_CTRL_T_OPTS+x}" ]; then
    printf '%s' "$FZF_CTRL_T_OPTS"
  else
    printf '%s' "--preview 'file {}'"
  fi
}

__yuru_ctrl_r_opts__() {
  printf '%s' "${YURU_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}"
}

__yuru_alt_c_opts__() {
  if [ "${YURU_ALT_C_OPTS+x}" ]; then
    printf '%s' "$YURU_ALT_C_OPTS"
  elif [ "${FZF_ALT_C_OPTS+x}" ]; then
    printf '%s' "$FZF_ALT_C_OPTS"
  else
    printf '%s' "--preview 'ls -la {} 2>/dev/null | head -100'"
  fi
}

__yuru_run_with_optional_command__() {
  local command_set="$1" command_text="$2" status
  shift 2
  if [ "$command_set" = 1 ]; then
    eval "$command_text" 2>/dev/null | "${YURU_BIN:-yuru}" "$@"
    status=${PIPESTATUS[1]}
    return $status
  fi
  "${YURU_BIN:-yuru}" "$@"
}

__yuru_compgen_path__() {
  local root="${1:-.}"
  if command -v fd >/dev/null 2>&1; then
    command fd --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  elif command -v fdfind >/dev/null 2>&1; then
    command fdfind --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  else
    command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o \( -type f -o -type d -o -type l \) -print 2>/dev/null | command sed 's#^\./##'
  fi
}

__yuru_compgen_dir__() {
  local root="${1:-.}"
  if command -v fd >/dev/null 2>&1; then
    command fd --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  elif command -v fdfind >/dev/null 2>&1; then
    command fdfind --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  else
    command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o -type d -print 2>/dev/null | command sed 's#^\./##'
  fi
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
  local command_set=1 command_text='__yuru_compgen_path__ .' selected insert opts
  local -a opt_args
  if [ "${YURU_CTRL_T_COMMAND+x}" ]; then
    command_text=$YURU_CTRL_T_COMMAND
  elif [ "${FZF_CTRL_T_COMMAND+x}" ]; then
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=$(__yuru_ctrl_t_opts__)
  opt_args=()
  [ -n "$opts" ] && eval "opt_args=($opts)"
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --fzf-compat ignore "${opt_args[@]}")
  [ -n "$selected" ] || return
  insert=$(__yuru_join_bash__ "$selected")
  [ -n "$insert" ] || return
  insert="$insert "
  READLINE_LINE="${READLINE_LINE:0:READLINE_POINT}${insert}${READLINE_LINE:READLINE_POINT}"
  READLINE_POINT=$((READLINE_POINT + ${#insert}))
}

__yuru_ctrl_r__() {
  local selected opts tmp status
  local -a opt_args
  opts=$(__yuru_ctrl_r_opts__)
  opt_args=()
  [ -n "$opts" ] && eval "opt_args=($opts)"
  tmp="${TMPDIR:-/tmp}/yuru-history.$$"
  rm -f "$tmp"
  HISTTIMEFORMAT= history | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//' >"$tmp" || { rm -f "$tmp"; return; }
  selected=$("${YURU_BIN:-yuru}" --scheme history --tac --no-sort --no-multi --query "$READLINE_LINE" --input "$tmp" --fzf-compat ignore "${opt_args[@]}")
  status=$?
  rm -f "$tmp"
  [ "$status" -eq 0 ] || return
  [ -n "$selected" ] || return
  READLINE_LINE=$selected
  READLINE_POINT=${#READLINE_LINE}
}

__yuru_alt_c__() {
  local command_set=1 command_text='__yuru_compgen_dir__ .' selected opts
  local -a opt_args
  if [ "${YURU_ALT_C_COMMAND+x}" ]; then
    command_text=$YURU_ALT_C_COMMAND
  elif [ "${FZF_ALT_C_COMMAND+x}" ]; then
    command_text=$FZF_ALT_C_COMMAND
  fi
  [ "$command_set" = 1 ] && [ -z "$command_text" ] && return

  opts=$(__yuru_alt_c_opts__)
  opt_args=()
  [ -n "$opts" ] && eval "opt_args=($opts)"
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --fzf-compat ignore "${opt_args[@]}")
  [ -n "$selected" ] || return
  builtin cd -- "$selected" || return
  READLINE_LINE=
  READLINE_POINT=0
}

__yuru_completion__() {
  local token trigger base dir root query selected insert opts walker multi
  local -a opt_args
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
  opt_args=()
  [ -n "$opts" ] && eval "opt_args=($opts)"
  if __yuru_completion_dirs_only__; then
    walker=dir,hidden
    multi=--no-multi
  else
    walker=file,dir,hidden
    multi=-m
  fi

  if __yuru_completion_dirs_only__; then
    selected=$(__yuru_compgen_dir__ "$root" | "${YURU_BIN:-yuru}" --scheme path $multi --query "$query" --fzf-compat ignore "${opt_args[@]}")
  else
    selected=$(__yuru_compgen_path__ "$root" | "${YURU_BIN:-yuru}" --scheme path $multi --query "$query" --fzf-compat ignore "${opt_args[@]}")
  fi
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

if __yuru_binding_enabled__ ctrl-t; then
  bind -x '"\C-t": __yuru_ctrl_t__'
fi
if __yuru_binding_enabled__ ctrl-r; then
  bind -x '"\C-r": __yuru_ctrl_r__'
fi
if __yuru_binding_enabled__ alt-c; then
  bind -x '"\ec": __yuru_alt_c__'
fi
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
if __yuru_binding_enabled__ completion; then
  __yuru_setup_completion__
fi
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

__yuru_binding_enabled__() {
  emulate -L zsh
  local name="$1" bindings=",${YURU_SHELL_BINDINGS:-all},"
  bindings="${bindings// /,}"
  case "$bindings" in
    *,all,*) return 0 ;;
    *,none,*) return 1 ;;
    *,"$name",*) return 0 ;;
  esac
  if [[ "$name" == completion ]]; then
    case "$bindings" in
      *,tab,* | *,path-completion,*) return 0 ;;
    esac
  fi
  return 1
}

__yuru_ctrl_t_opts__() {
  emulate -L zsh
  if (( ${+YURU_CTRL_T_OPTS} )); then
    print -rn -- "$YURU_CTRL_T_OPTS"
  elif (( ${+FZF_CTRL_T_OPTS} )); then
    print -rn -- "$FZF_CTRL_T_OPTS"
  else
    print -rn -- "--preview 'file {}'"
  fi
}

__yuru_ctrl_r_opts__() {
  emulate -L zsh
  print -rn -- "${YURU_CTRL_R_OPTS:-${FZF_CTRL_R_OPTS:-}}"
}

__yuru_alt_c_opts__() {
  emulate -L zsh
  if (( ${+YURU_ALT_C_OPTS} )); then
    print -rn -- "$YURU_ALT_C_OPTS"
  elif (( ${+FZF_ALT_C_OPTS} )); then
    print -rn -- "$FZF_ALT_C_OPTS"
  else
    print -rn -- "--preview 'ls -la {} 2>/dev/null | head -100'"
  fi
}

__yuru_run_with_optional_command__() {
  emulate -L zsh
  local command_set="$1" command_text="$2" yuru_status
  shift 2
  if [[ "$command_set" == 1 ]]; then
    eval "$command_text" 2>/dev/null | "${YURU_BIN:-yuru}" "$@"
    yuru_status=${pipestatus[2]}
    return $yuru_status
  fi
  "${YURU_BIN:-yuru}" "$@"
}

__yuru_compgen_path__() {
  emulate -L zsh
  local root="${1:-.}"
  if command -v fd >/dev/null 2>&1; then
    command fd --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  elif command -v fdfind >/dev/null 2>&1; then
    command fdfind --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  else
    command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o \( -type f -o -type d -o -type l \) -print 2>/dev/null | command sed 's#^\./##'
  fi
}

__yuru_compgen_dir__() {
  emulate -L zsh
  local root="${1:-.}"
  if command -v fd >/dev/null 2>&1; then
    command fd --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  elif command -v fdfind >/dev/null 2>&1; then
    command fdfind --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
  else
    command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o -type d -print 2>/dev/null | command sed 's#^\./##'
  fi
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
  local command_set=1 command_text='__yuru_compgen_path__ .' selected insert opts
  if (( ${+YURU_CTRL_T_COMMAND} )); then
    command_text=$YURU_CTRL_T_COMMAND
  elif (( ${+FZF_CTRL_T_COMMAND} )); then
    command_text=$FZF_CTRL_T_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=$(__yuru_ctrl_t_opts__)
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --fzf-compat ignore ${(@Q)${(z)opts}})
  [[ -n "$selected" ]] || return
  insert=$(__yuru_join_zsh__ "$selected")
  [[ -n "$insert" ]] || return
  LBUFFER="${LBUFFER}${insert} "
  zle reset-prompt
}

__yuru_ctrl_r__() {
  emulate -L zsh
  local selected opts tmp yuru_status
  opts=$(__yuru_ctrl_r_opts__)
  tmp="${TMPDIR:-/tmp}/yuru-history.$$"
  rm -f "$tmp"
  fc -rl 1 | sed 's/^[[:space:]]*[0-9][0-9]*[[:space:]]*//' >"$tmp" || { rm -f "$tmp"; return }
  selected=$("${YURU_BIN:-yuru}" --scheme history --tac --no-sort --no-multi --query "$LBUFFER" --input "$tmp" --fzf-compat ignore ${(@Q)${(z)opts}})
  yuru_status=$?
  rm -f "$tmp"
  (( yuru_status == 0 )) || return
  [[ -n "$selected" ]] || return
  BUFFER=$selected
  CURSOR=${#BUFFER}
  zle reset-prompt
}

__yuru_alt_c__() {
  emulate -L zsh
  local command_set=1 command_text='__yuru_compgen_dir__ .' selected opts
  if (( ${+YURU_ALT_C_COMMAND} )); then
    command_text=$YURU_ALT_C_COMMAND
  elif (( ${+FZF_ALT_C_COMMAND} )); then
    command_text=$FZF_ALT_C_COMMAND
  fi
  [[ "$command_set" == 1 && -z "$command_text" ]] && return

  opts=$(__yuru_alt_c_opts__)
  selected=$(__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --fzf-compat ignore ${(@Q)${(z)opts}})
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
    walker=dir,hidden
    multi=--no-multi
  else
    walker=file,dir,hidden
    multi=-m
  fi

  if __yuru_completion_dirs_only__; then
    selected=$(__yuru_compgen_dir__ "$root" | "${YURU_BIN:-yuru}" --scheme path $multi --query "$query" --fzf-compat ignore ${(@Q)${(z)opts}})
  else
    selected=$(__yuru_compgen_path__ "$root" | "${YURU_BIN:-yuru}" --scheme path $multi --query "$query" --fzf-compat ignore ${(@Q)${(z)opts}})
  fi
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
if __yuru_binding_enabled__ ctrl-t; then
  bindkey -M emacs '^T' __yuru_ctrl_t__
  bindkey -M viins '^T' __yuru_ctrl_t__
  bindkey -M vicmd '^T' __yuru_ctrl_t__
fi
if __yuru_binding_enabled__ ctrl-r; then
  bindkey -M emacs '^R' __yuru_ctrl_r__
  bindkey -M viins '^R' __yuru_ctrl_r__
  bindkey -M vicmd '^R' __yuru_ctrl_r__
fi
if __yuru_binding_enabled__ alt-c; then
  bindkey -M emacs '^[c' __yuru_alt_c__
  bindkey -M viins '^[c' __yuru_alt_c__
  bindkey -M vicmd '^[c' __yuru_alt_c__
fi
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
if __yuru_binding_enabled__ completion; then
  bindkey -M emacs '^I' __yuru_completion__
  bindkey -M viins '^I' __yuru_completion__
fi
"#;

const FISH: &str = r#"# yuru shell integration for fish
# Install with: yuru --fish | source

function __yuru_join_fish__
    string split \n -- $argv[1] | string match -v '' | string escape | string join ' '
end

function __yuru_binding_enabled__
    set -l name $argv[1]
    set -l bindings all
    if set -q YURU_SHELL_BINDINGS
        set bindings (string replace -a ' ' ',' -- (string lower -- $YURU_SHELL_BINDINGS))
    end
    set -l wrapped ",$bindings,"
    if string match -q '*,all,*' -- $wrapped
        return 0
    end
    if string match -q '*,none,*' -- $wrapped
        return 1
    end
    if string match -q "*,$name,*" -- $wrapped
        return 0
    end
    if test "$name" = completion
        if string match -q '*,tab,*' -- $wrapped; or string match -q '*,path-completion,*' -- $wrapped
            return 0
        end
    end
    return 1
end

function __yuru_split_opts__
    set -l raw $argv[1]
    set -l opts
    if test -n "$raw"
        eval "set opts $raw"
    end
    printf '%s\n' $opts
end

function __yuru_ctrl_t_opts__
    if set -q YURU_CTRL_T_OPTS
        __yuru_split_opts__ "$YURU_CTRL_T_OPTS"
    else if set -q FZF_CTRL_T_OPTS
        __yuru_split_opts__ "$FZF_CTRL_T_OPTS"
    else
        printf '%s\n' --preview 'file {}'
    end
end

function __yuru_ctrl_r_opts__
    if set -q YURU_CTRL_R_OPTS
        __yuru_split_opts__ "$YURU_CTRL_R_OPTS"
    else if set -q FZF_CTRL_R_OPTS
        __yuru_split_opts__ "$FZF_CTRL_R_OPTS"
    end
end

function __yuru_alt_c_opts__
    if set -q YURU_ALT_C_OPTS
        __yuru_split_opts__ "$YURU_ALT_C_OPTS"
    else if set -q FZF_ALT_C_OPTS
        __yuru_split_opts__ "$FZF_ALT_C_OPTS"
    else
        printf '%s\n' --preview 'ls -la {} 2>/dev/null | head -100'
    end
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

function __yuru_compgen_path__
    set -l root .
    if test (count $argv) -gt 0; and test -n "$argv[1]"
        set root $argv[1]
    end
    if command -q fd
        command fd --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
    else if command -q fdfind
        command fdfind --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
    else
        command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o \( -type f -o -type d -o -type l \) -print 2>/dev/null | command sed 's#^\./##'
    end
end

function __yuru_compgen_dir__
    set -l root .
    if test (count $argv) -gt 0; and test -n "$argv[1]"
        set root $argv[1]
    end
    if command -q fd
        command fd --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
    else if command -q fdfind
        command fdfind --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . "$root"
    else
        command find "$root" -mindepth 1 \( -name .git -o -name node_modules -o -name target -o -name Library -o -name .rustup -o -name .bun -o -name .cache -o -name .cargo -o -name .npm -o -name .vscode -o -name .Trash \) -prune -o -type d -print 2>/dev/null | command sed 's#^\./##'
    end
end

function __yuru_run_with_optional_command__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l command_set $argv[1]
    set -l command_text $argv[2]
    set -e argv[1]
    set -e argv[1]

    if test "$command_set" = 1
        eval $command_text 2>/dev/null | $yuru_bin $argv
        set -l pipe_status $pipestatus
        return $pipe_status[2]
    end

    $yuru_bin $argv
end

function __yuru_ctrl_t__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l command_set 1
    set -l command_text "__yuru_compgen_path__ ."
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
    set opts (__yuru_ctrl_t_opts__)
    set selected (__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path -m --fzf-compat ignore $opts)
    set -q selected[1]; or return
    set -l insert (__yuru_join_fish__ (string join \n -- $selected))
    commandline -i "$insert "
    commandline -f repaint
end

function __yuru_ctrl_r__
    set -l yuru_bin (set -q YURU_BIN; and echo $YURU_BIN; or echo yuru)
    set -l opts
    set opts (__yuru_ctrl_r_opts__)
    set -l tmpdir /tmp
    if set -q TMPDIR; and test -n "$TMPDIR"
        set tmpdir $TMPDIR
    end
    set -l tmp (mktemp "$tmpdir/yuru-history.XXXXXX")
    history >$tmp
    set -l selected ($yuru_bin --scheme history --tac --no-sort --no-multi --query (commandline) --input "$tmp" --fzf-compat ignore $opts)
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
    set -l command_set 1
    set -l command_text "__yuru_compgen_dir__ ."
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
    set opts (__yuru_alt_c_opts__)
    set selected (__yuru_run_with_optional_command__ "$command_set" "$command_text" --scheme path --no-multi --fzf-compat ignore $opts)
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
        set selected (__yuru_compgen_dir__ "$root" | $yuru_bin --scheme path --no-multi --query "$query" --fzf-compat ignore $opts)
    else
        set selected (__yuru_compgen_path__ "$root" | $yuru_bin --scheme path -m --query "$query" --fzf-compat ignore $opts)
    end
    set -q selected[1]; or return
    set -l insert (__yuru_join_fish__ (string join \n -- $selected))
    commandline --current-token --replace "$insert "
    commandline -f repaint
end

if __yuru_binding_enabled__ ctrl-t
    bind \ct __yuru_ctrl_t__
end
if __yuru_binding_enabled__ ctrl-r
    bind \cr __yuru_ctrl_r__
end
if __yuru_binding_enabled__ alt-c
    bind \ec __yuru_alt_c__
end
# fzf-style path completion trigger: COMMAND [FUZZY]**<TAB>
if __yuru_binding_enabled__ completion
    bind \t __yuru_completion__
end
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

function Test-YuruBindingEnabled {
    param([string]$Name)
    $bindings = "all"
    if ($env:YURU_SHELL_BINDINGS) { $bindings = $env:YURU_SHELL_BINDINGS.ToLowerInvariant() }
    $items = @($bindings -split '[,\s]+' | Where-Object { $_ })
    if ($items -contains "all") { return $true }
    if ($items -contains "none") { return $false }
    if ($items -contains $Name) { return $true }
    if ($Name -eq "completion" -and (($items -contains "tab") -or ($items -contains "path-completion"))) {
        return $true
    }
    return $false
}

function Split-YuruOptions {
    param([string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return @() }
    $errors = $null
    $tokens = [System.Management.Automation.PSParser]::Tokenize($Value, [ref]$errors)
    if ($errors -and $errors.Count -gt 0) {
        return @($Value -split '\s+' | Where-Object { $_ })
    }
    return @(
        $tokens |
            Where-Object { $_.Type -in @("CommandArgument", "String", "Number") } |
            ForEach-Object { $_.Content }
    )
}

function Get-YuruCtrlTOptions {
    if ($env:YURU_CTRL_T_OPTS) { return @(Split-YuruOptions $env:YURU_CTRL_T_OPTS) }
    if ($env:FZF_CTRL_T_OPTS) { return @(Split-YuruOptions $env:FZF_CTRL_T_OPTS) }
    return @("--preview", "Get-Item -LiteralPath {} | Format-List | Out-String")
}

function Get-YuruCtrlROptions {
    if ($env:YURU_CTRL_R_OPTS) { return @(Split-YuruOptions $env:YURU_CTRL_R_OPTS) }
    if ($env:FZF_CTRL_R_OPTS) { return @(Split-YuruOptions $env:FZF_CTRL_R_OPTS) }
    return @()
}

function Get-YuruAltCOptions {
    if ($env:YURU_ALT_C_OPTS) { return @(Split-YuruOptions $env:YURU_ALT_C_OPTS) }
    if ($env:FZF_ALT_C_OPTS) { return @(Split-YuruOptions $env:FZF_ALT_C_OPTS) }
    return @("--preview", "Get-ChildItem -Force -LiteralPath {} | Select-Object -First 100 | Out-String")
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

function Get-YuruPathItems {
    param([string]$Root = ".")
    $fd = Get-Command fd -ErrorAction SilentlyContinue
    if (-not $fd) { $fd = Get-Command fdfind -ErrorAction SilentlyContinue }
    if ($fd) {
        & $fd.Source --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . $Root
        return
    }
    Get-ChildItem -LiteralPath $Root -Force -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notin @(".git", "node_modules", "target", "Library", ".rustup", ".bun", ".cache", ".cargo", ".npm", ".vscode", ".Trash") } |
        ForEach-Object {
            $relative = Resolve-Path -LiteralPath $_.FullName -Relative -ErrorAction SilentlyContinue
            if ($relative) { $relative -replace '^\.[\\/]', '' }
        }
}

function Get-YuruDirItems {
    param([string]$Root = ".")
    $fd = Get-Command fd -ErrorAction SilentlyContinue
    if (-not $fd) { $fd = Get-Command fdfind -ErrorAction SilentlyContinue }
    if ($fd) {
        & $fd.Source --type d --hidden --exclude .git --exclude node_modules --exclude target --exclude Library --exclude .rustup --exclude .bun --exclude .cache --exclude .cargo --exclude .npm --exclude .vscode --exclude .Trash --exclude .local/share --exclude go/pkg/mod . $Root
        return
    }
    Get-ChildItem -LiteralPath $Root -Force -Directory -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notin @(".git", "node_modules", "target", "Library", ".rustup", ".bun", ".cache", ".cargo", ".npm", ".vscode", ".Trash") } |
        ForEach-Object {
            $relative = Resolve-Path -LiteralPath $_.FullName -Relative -ErrorAction SilentlyContinue
            if ($relative) { $relative -replace '^\.[\\/]', '' }
        }
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
        try {
            return @(Invoke-Expression $CommandText 2>$null | & $yuru @YuruArgs)
        } catch {
            return @()
        }
    }
    & $yuru @YuruArgs
}

function Invoke-YuruCtrlT {
    $commandText = "Get-YuruPathItems ."
    if (Test-Path Env:YURU_CTRL_T_COMMAND) {
        $commandText = $env:YURU_CTRL_T_COMMAND
    } elseif (Test-Path Env:FZF_CTRL_T_COMMAND) {
        $commandText = $env:FZF_CTRL_T_COMMAND
    }
    $opts = @(Get-YuruCtrlTOptions)
    $yuruArgs = @("--scheme", "path", "-m", "--fzf-compat", "ignore") + $opts
    $selected = @(Invoke-YuruWithOptionalCommand -CommandText $commandText -YuruArgs $yuruArgs)
    if ($selected.Count -eq 0) { return }
    $insert = Join-YuruSelection $selected
    if ([string]::IsNullOrEmpty($insert)) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($insert + " ")
}

function Invoke-YuruCtrlR {
    $yuru = Get-YuruCommand
    $opts = @(Get-YuruCtrlROptions)
    $line = $null
    $cursor = $null
    [Microsoft.PowerShell.PSConsoleReadLine]::GetBufferState([ref]$line, [ref]$cursor)
    $yuruArgs = @("--scheme", "history", "--tac", "--no-sort", "--no-multi", "--query", $line, "--fzf-compat", "ignore") + $opts
    $selected = @(Invoke-YuruWithItems -Items @(Get-YuruHistoryLines) -YuruArgs $yuruArgs | Select-Object -First 1)
    if ($selected.Count -eq 0 -or [string]::IsNullOrEmpty($selected[0])) { return }
    [Microsoft.PowerShell.PSConsoleReadLine]::RevertLine()
    [Microsoft.PowerShell.PSConsoleReadLine]::Insert($selected[0])
}

function Invoke-YuruAltC {
    $commandText = "Get-YuruDirItems ."
    if (Test-Path Env:YURU_ALT_C_COMMAND) {
        $commandText = $env:YURU_ALT_C_COMMAND
    } elseif (Test-Path Env:FZF_ALT_C_COMMAND) {
        $commandText = $env:FZF_ALT_C_COMMAND
    }
    $opts = @(Get-YuruAltCOptions)
    $yuruArgs = @("--scheme", "path", "--no-multi", "--fzf-compat", "ignore") + $opts
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
        $selected = @(Get-YuruDirItems $root | & $yuru --scheme path --no-multi --query $query --fzf-compat ignore @opts)
    } else {
        $selected = @(Get-YuruPathItems $root | & $yuru --scheme path -m --query $query --fzf-compat ignore @opts)
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
    if (Test-YuruBindingEnabled "ctrl-t") {
        Set-PSReadLineKeyHandler -Key Ctrl+t -ScriptBlock { Invoke-YuruCtrlT }
    }
    if (Test-YuruBindingEnabled "ctrl-r") {
        Set-PSReadLineKeyHandler -Key Ctrl+r -ScriptBlock { Invoke-YuruCtrlR }
    }
    if (Test-YuruBindingEnabled "alt-c") {
        Set-PSReadLineKeyHandler -Key Alt+c -ScriptBlock { Invoke-YuruAltC }
    }
    # fzf-style path completion trigger: COMMAND [FUZZY]**<Tab>
    if (Test-YuruBindingEnabled "completion") {
        Set-PSReadLineKeyHandler -Key Tab -ScriptBlock { param($key, $arg) Invoke-YuruCompletion $key $arg }
    }
}
"#;
