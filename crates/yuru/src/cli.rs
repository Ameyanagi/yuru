use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};

pub(crate) const DEFAULT_INTERACTIVE_LIMIT: usize = 1000;
pub(crate) const DEFAULT_PREVIEW_TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rst", "toml", "json", "jsonl", "yaml", "yml", "csv", "tsv", "log",
    "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "c", "h", "cpp", "hpp", "cs", "rb", "php",
    "sh", "bash", "zsh", "fish", "ps1", "sql", "html", "htm", "css", "scss", "xml",
];

const DEFAULT_WALKER: &str = "file,follow,hidden";
const DEFAULT_WALKER_ROOT: &str = ".";
const DEFAULT_WALKER_SKIP: &str = ".git,node_modules";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum LangArg {
    Plain,
    Ja,
    Ko,
    Zh,
    All,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum SchemeArg {
    Default,
    Path,
    History,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum FzfCompatArg {
    Strict,
    Warn,
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum LoadFzfDefaultOptsArg {
    Never,
    Safe,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum JaReadingArg {
    None,
    Lindera,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ZhPolyphoneArg {
    None,
    Common,
    #[value(hide = true)]
    Phrase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ZhScriptArg {
    Auto,
    Hans,
    Hant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PreviewImageProtocolArg {
    None,
    Halfblocks,
    Sixel,
    Kitty,
    Iterm2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum AlgoArg {
    Greedy,
    #[value(alias = "v1")]
    FzfV1,
    #[value(alias = "v2")]
    FzfV2,
    Nucleo,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandArg {
    /// Reconfigure user defaults interactively.
    Configure,

    /// Print environment, config, and shell integration diagnostics.
    Doctor,

    /// Parse shell words for generated shell integrations.
    #[command(name = "__split-shell-words", hide = true)]
    SplitShellWords {
        #[arg(allow_hyphen_values = true)]
        words: String,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "yuru",
    about = "A fast phonetic fuzzy finder for multilingual shell workflows",
    version,
    args_override_self = true
)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Option<CommandArg>,

    #[arg(long, value_enum, default_value_t = LangArg::Plain)]
    pub(crate) lang: LangArg,

    #[arg(long = "ja-reading", value_enum, default_value_t = JaReadingArg::Lindera)]
    pub(crate) ja_reading: JaReadingArg,

    #[arg(long = "zh-pinyin", default_value_t = true)]
    pub(crate) zh_pinyin: bool,

    #[arg(long = "no-zh-pinyin")]
    pub(crate) no_zh_pinyin: bool,

    #[arg(long = "zh-initials", default_value_t = true)]
    pub(crate) zh_initials: bool,

    #[arg(long = "no-zh-initials")]
    pub(crate) no_zh_initials: bool,

    #[arg(long = "zh-polyphone", value_enum, default_value_t = ZhPolyphoneArg::Common)]
    pub(crate) zh_polyphone: ZhPolyphoneArg,

    #[arg(long = "zh-script", value_enum, default_value_t = ZhScriptArg::Auto, hide = true)]
    pub(crate) zh_script: ZhScriptArg,

    #[arg(long = "ko-romanization", default_value_t = true)]
    pub(crate) ko_romanization: bool,

    #[arg(long = "no-ko-romanization")]
    pub(crate) no_ko_romanization: bool,

    #[arg(long = "ko-initials", default_value_t = true)]
    pub(crate) ko_initials: bool,

    #[arg(long = "no-ko-initials")]
    pub(crate) no_ko_initials: bool,

    #[arg(long = "ko-keyboard", default_value_t = true)]
    pub(crate) ko_keyboard: bool,

    #[arg(long = "no-ko-keyboard")]
    pub(crate) no_ko_keyboard: bool,

    #[arg(short = 'q', long)]
    pub(crate) query: Option<String>,

    #[arg(short = 'f', long)]
    pub(crate) filter: Option<String>,

    #[arg(long)]
    pub(crate) limit: Option<usize>,

    #[arg(long, default_value_t = 8)]
    pub(crate) max_query_variants: usize,

    #[arg(long, default_value_t = 8)]
    pub(crate) max_keys_per_candidate: usize,

    #[arg(long, default_value_t = 1024)]
    pub(crate) max_total_key_bytes_per_candidate: usize,

    #[arg(long, default_value_t = 1000)]
    pub(crate) top_b: usize,

    #[arg(short = 'e', long)]
    pub(crate) exact: bool,

    #[arg(long = "no-exact")]
    pub(crate) no_exact: bool,

    #[arg(long = "extended-exact")]
    pub(crate) extended_exact: bool,

    #[arg(short = 'x', long, default_value_t = true)]
    pub(crate) extended: bool,

    #[arg(long = "no-extended")]
    pub(crate) no_extended: bool,

    #[arg(short = 'i', long)]
    pub(crate) ignore_case: bool,

    #[arg(long = "no-ignore-case")]
    pub(crate) no_ignore_case: bool,

    #[arg(long, default_value_t = true)]
    pub(crate) smart_case: bool,

    #[arg(long)]
    pub(crate) no_sort: bool,

    #[arg(short = 's', long, num_args = 0..=1)]
    pub(crate) sort: Option<Option<usize>>,

    #[arg(long, default_value = "length")]
    pub(crate) tiebreak: String,

    #[arg(long, value_enum, default_value_t = SchemeArg::Default)]
    pub(crate) scheme: SchemeArg,

    #[arg(long)]
    pub(crate) disabled: bool,

    #[arg(long)]
    pub(crate) phony: bool,

    #[arg(long)]
    pub(crate) enabled: bool,

    #[arg(long = "no-phony")]
    pub(crate) no_phony: bool,

    #[arg(long)]
    pub(crate) literal: bool,

    #[arg(long = "no-literal")]
    pub(crate) no_literal: bool,

    #[arg(long)]
    pub(crate) tac: bool,

    #[arg(long = "no-tac")]
    pub(crate) no_tac: bool,

    #[arg(long)]
    pub(crate) tail: Option<usize>,

    #[arg(long = "no-tail")]
    pub(crate) no_tail: bool,

    #[arg(long)]
    pub(crate) read0: bool,

    #[arg(long = "no-read0")]
    pub(crate) no_read0: bool,

    #[arg(long)]
    pub(crate) sync: bool,

    #[arg(long = "no-sync", alias = "async")]
    pub(crate) no_sync: bool,

    #[arg(long)]
    pub(crate) print0: bool,

    #[arg(long = "no-print0")]
    pub(crate) no_print0: bool,

    #[arg(long, hide = true)]
    pub(crate) input: Option<PathBuf>,

    #[arg(long)]
    pub(crate) ansi: bool,

    #[arg(long = "no-ansi")]
    pub(crate) no_ansi: bool,

    #[arg(long)]
    pub(crate) print_query: bool,

    #[arg(long = "no-print-query")]
    pub(crate) no_print_query: bool,

    #[arg(short = '1', long)]
    pub(crate) select_1: bool,

    #[arg(long = "no-select-1")]
    pub(crate) no_select_1: bool,

    #[arg(short = '0', long)]
    pub(crate) exit_0: bool,

    #[arg(long = "no-exit-0")]
    pub(crate) no_exit_0: bool,

    #[arg(short = 'n', long)]
    pub(crate) nth: Option<String>,

    #[arg(long)]
    pub(crate) with_nth: Option<String>,

    #[arg(long)]
    pub(crate) accept_nth: Option<String>,

    #[arg(short = 'd', long)]
    pub(crate) delimiter: Option<String>,

    #[arg(
        long,
        value_enum,
        default_value_t = AlgoArg::Greedy,
        help = "Matcher backend: greedy/fzf-v1 use Yuru scoring; fzf-v2/nucleo use nucleo scoring",
        long_help = "Matcher backend. greedy and fzf-v1 use Yuru's greedy scorer. fzf-v2 and nucleo use the nucleo-backed quality scorer. The fzf names are compatibility-inspired modes, not byte-for-byte fzf algorithm implementations; normal nucleo searches parallelize on large inputs, while extended-syntax nucleo searches can still be slower."
    )]
    pub(crate) algo: AlgoArg,

    #[arg(long = "fzf-compat", value_enum)]
    pub(crate) fzf_compat: Option<FzfCompatArg>,

    #[arg(long = "load-fzf-default-opts", value_enum, default_value_t = LoadFzfDefaultOptsArg::Safe)]
    pub(crate) load_fzf_default_opts: LoadFzfDefaultOptsArg,

    #[arg(short = 'm', long, num_args = 0..=1)]
    pub(crate) multi: Option<Option<usize>>,

    #[arg(long)]
    pub(crate) no_multi: bool,

    #[arg(long)]
    pub(crate) expect: Option<String>,

    #[arg(long = "no-expect")]
    pub(crate) no_expect: bool,

    #[arg(long)]
    pub(crate) bind: Vec<String>,

    #[arg(long = "toggle-sort")]
    pub(crate) toggle_sort: Option<String>,

    #[arg(long)]
    pub(crate) preview: Option<String>,

    #[arg(long = "no-preview")]
    pub(crate) no_preview: bool,

    #[arg(long = "preview-auto")]
    pub(crate) preview_auto: bool,

    #[arg(long = "preview-text-extensions")]
    pub(crate) preview_text_extensions: Option<String>,

    #[arg(long = "preview-image-protocol", value_enum, default_value_t = PreviewImageProtocolArg::None)]
    pub(crate) preview_image_protocol: PreviewImageProtocolArg,

    #[arg(long)]
    pub(crate) preview_window: Option<String>,

    #[arg(long, num_args = 0..=1)]
    pub(crate) preview_border: Option<Option<String>>,

    #[arg(long = "no-preview-border")]
    pub(crate) no_preview_border: bool,

    #[arg(long)]
    pub(crate) preview_label: Option<String>,

    #[arg(long)]
    pub(crate) preview_label_pos: Option<String>,

    #[arg(long)]
    pub(crate) preview_wrap_sign: Option<String>,

    #[arg(long)]
    pub(crate) height: Option<String>,

    #[arg(long)]
    pub(crate) no_height: bool,

    #[arg(long)]
    pub(crate) min_height: Option<String>,

    #[arg(long, num_args = 0..=1)]
    pub(crate) popup: Option<Option<String>>,

    #[arg(long = "no-popup")]
    pub(crate) no_popup: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) tmux: Option<Option<String>>,

    #[arg(long = "no-tmux")]
    pub(crate) no_tmux: bool,

    #[arg(long)]
    pub(crate) layout: Option<String>,

    #[arg(long)]
    pub(crate) reverse: bool,

    #[arg(long = "no-reverse")]
    pub(crate) no_reverse: bool,

    #[arg(long)]
    pub(crate) margin: Option<String>,

    #[arg(long)]
    pub(crate) padding: Option<String>,

    #[arg(long = "no-margin")]
    pub(crate) no_margin: bool,

    #[arg(long = "no-padding")]
    pub(crate) no_padding: bool,

    #[arg(long = "no-border")]
    pub(crate) no_border: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) border: Option<Option<String>>,

    #[arg(long)]
    pub(crate) border_label: Option<String>,

    #[arg(long)]
    pub(crate) border_label_pos: Option<String>,

    #[arg(long = "no-border-label")]
    pub(crate) no_border_label: bool,

    #[arg(long)]
    pub(crate) prompt: Option<String>,

    #[arg(long)]
    pub(crate) header: Option<String>,

    #[arg(long = "no-header")]
    pub(crate) no_header: bool,

    #[arg(long)]
    pub(crate) header_lines: Option<usize>,

    #[arg(long = "no-header-lines")]
    pub(crate) no_header_lines: bool,

    #[arg(long)]
    pub(crate) header_first: bool,

    #[arg(long = "no-header-first")]
    pub(crate) no_header_first: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) header_border: Option<Option<String>>,

    #[arg(long = "no-header-border")]
    pub(crate) no_header_border: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) header_lines_border: Option<Option<String>>,

    #[arg(long = "no-header-lines-border")]
    pub(crate) no_header_lines_border: bool,

    #[arg(long)]
    pub(crate) header_label: Option<String>,

    #[arg(long)]
    pub(crate) header_label_pos: Option<String>,

    #[arg(long = "no-header-label")]
    pub(crate) no_header_label: bool,

    #[arg(long)]
    pub(crate) footer: Option<String>,

    #[arg(long = "no-footer")]
    pub(crate) no_footer: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) footer_border: Option<Option<String>>,

    #[arg(long = "no-footer-border")]
    pub(crate) no_footer_border: bool,

    #[arg(long)]
    pub(crate) footer_label: Option<String>,

    #[arg(long)]
    pub(crate) footer_label_pos: Option<String>,

    #[arg(long = "no-footer-label")]
    pub(crate) no_footer_label: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) color: Vec<Option<String>>,

    #[arg(long)]
    pub(crate) no_color: bool,

    #[arg(long = "no-256")]
    pub(crate) no_256: bool,

    #[arg(long)]
    pub(crate) bold: bool,

    #[arg(long)]
    pub(crate) no_bold: bool,

    #[arg(long)]
    pub(crate) black: bool,

    #[arg(long = "no-black")]
    pub(crate) no_black: bool,

    #[arg(long)]
    pub(crate) cycle: bool,

    #[arg(long = "no-cycle")]
    pub(crate) no_cycle: bool,

    #[arg(long)]
    pub(crate) highlight_line: bool,

    #[arg(long = "no-highlight-line")]
    pub(crate) no_highlight_line: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) wrap: Option<Option<String>>,

    #[arg(long = "no-wrap")]
    pub(crate) no_wrap: bool,

    #[arg(long = "wrap-word")]
    pub(crate) wrap_word: bool,

    #[arg(long = "no-wrap-word")]
    pub(crate) no_wrap_word: bool,

    #[arg(long)]
    pub(crate) wrap_sign: Option<String>,

    #[arg(long = "multi-line")]
    pub(crate) multi_line: bool,

    #[arg(long)]
    pub(crate) no_multi_line: bool,

    #[arg(long)]
    pub(crate) raw: bool,

    #[arg(long = "no-raw")]
    pub(crate) no_raw: bool,

    #[arg(long)]
    pub(crate) track: bool,

    #[arg(long = "no-track")]
    pub(crate) no_track: bool,

    #[arg(long)]
    pub(crate) id_nth: Option<String>,

    #[arg(long = "no-id-nth")]
    pub(crate) no_id_nth: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) gap: Option<Option<usize>>,

    #[arg(long = "no-gap")]
    pub(crate) no_gap: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) gap_line: Option<Option<String>>,

    #[arg(long = "no-gap-line")]
    pub(crate) no_gap_line: bool,

    #[arg(long)]
    pub(crate) freeze_left: Option<usize>,

    #[arg(long)]
    pub(crate) freeze_right: Option<usize>,

    #[arg(long)]
    pub(crate) keep_right: bool,

    #[arg(long = "no-keep-right")]
    pub(crate) no_keep_right: bool,

    #[arg(long)]
    pub(crate) scroll_off: Option<usize>,

    #[arg(long)]
    pub(crate) no_hscroll: bool,

    #[arg(long)]
    pub(crate) hscroll: bool,

    #[arg(long)]
    pub(crate) hscroll_off: Option<usize>,

    #[arg(long)]
    pub(crate) jump_labels: Option<String>,

    #[arg(long)]
    pub(crate) gutter: Option<String>,

    #[arg(long)]
    pub(crate) gutter_raw: Option<String>,

    #[arg(long)]
    pub(crate) pointer: Option<String>,

    #[arg(long)]
    pub(crate) marker: Option<String>,

    #[arg(long)]
    pub(crate) marker_multi_line: Option<String>,

    #[arg(long)]
    pub(crate) ellipsis: Option<String>,

    #[arg(long)]
    pub(crate) tabstop: Option<usize>,

    #[arg(long, num_args = 0..=1)]
    pub(crate) scrollbar: Option<Option<String>>,

    #[arg(long)]
    pub(crate) no_scrollbar: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) list_border: Option<Option<String>>,

    #[arg(long = "no-list-border")]
    pub(crate) no_list_border: bool,

    #[arg(long)]
    pub(crate) list_label: Option<String>,

    #[arg(long)]
    pub(crate) list_label_pos: Option<String>,

    #[arg(long = "no-list-label")]
    pub(crate) no_list_label: bool,

    #[arg(long)]
    pub(crate) no_input: bool,

    #[arg(long)]
    pub(crate) info: Option<String>,

    #[arg(long)]
    pub(crate) info_command: Option<String>,

    #[arg(long = "no-info-command")]
    pub(crate) no_info_command: bool,

    #[arg(long = "no-info")]
    pub(crate) no_info: bool,

    #[arg(long = "inline-info")]
    pub(crate) inline_info: bool,

    #[arg(long = "no-inline-info")]
    pub(crate) no_inline_info: bool,

    #[arg(long)]
    pub(crate) separator: Option<String>,

    #[arg(long)]
    pub(crate) no_separator: bool,

    #[arg(long)]
    pub(crate) ghost: Option<String>,

    #[arg(long)]
    pub(crate) filepath_word: bool,

    #[arg(long = "no-filepath-word")]
    pub(crate) no_filepath_word: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) input_border: Option<Option<String>>,

    #[arg(long = "no-input-border")]
    pub(crate) no_input_border: bool,

    #[arg(long)]
    pub(crate) input_label: Option<String>,

    #[arg(long)]
    pub(crate) input_label_pos: Option<String>,

    #[arg(long = "no-input-label")]
    pub(crate) no_input_label: bool,

    #[arg(long, default_value = DEFAULT_WALKER)]
    pub(crate) walker: String,

    #[arg(long = "walker-root", default_value = DEFAULT_WALKER_ROOT)]
    pub(crate) walker_roots: Vec<PathBuf>,

    #[arg(long = "walker-skip", default_value = DEFAULT_WALKER_SKIP)]
    pub(crate) walker_skip: String,

    #[arg(long)]
    pub(crate) with_shell: Option<String>,

    #[arg(long)]
    pub(crate) style: Option<String>,

    #[arg(long, num_args = 0..=1)]
    pub(crate) listen: Option<Option<String>>,

    #[arg(long = "no-listen")]
    pub(crate) no_listen: bool,

    #[arg(long, num_args = 0..=1)]
    pub(crate) listen_unsafe: Option<Option<String>>,

    #[arg(long = "no-listen-unsafe")]
    pub(crate) no_listen_unsafe: bool,

    #[arg(long)]
    pub(crate) history: Option<PathBuf>,

    #[arg(long = "no-history")]
    pub(crate) no_history: bool,

    #[arg(long)]
    pub(crate) history_size: Option<usize>,

    #[arg(long)]
    pub(crate) no_tty_default: bool,

    #[arg(long)]
    pub(crate) tty_default: Option<String>,

    #[arg(long = "force-tty-in")]
    pub(crate) force_tty_in: bool,

    #[arg(long = "no-force-tty-in")]
    pub(crate) no_force_tty_in: bool,

    #[arg(long = "proxy-script")]
    pub(crate) proxy_script: Option<String>,

    #[arg(long = "no-winpty")]
    pub(crate) no_winpty: bool,

    #[arg(long)]
    pub(crate) no_mouse: bool,

    #[arg(long)]
    pub(crate) no_unicode: bool,

    #[arg(long)]
    pub(crate) unicode: bool,

    #[arg(long)]
    pub(crate) ambidouble: bool,

    #[arg(long = "no-ambidouble")]
    pub(crate) no_ambidouble: bool,

    #[arg(long)]
    pub(crate) clear: bool,

    #[arg(long)]
    pub(crate) no_clear: bool,

    #[arg(long)]
    pub(crate) man: bool,

    #[arg(long)]
    pub(crate) threads: Option<usize>,

    #[arg(long)]
    pub(crate) bench: Option<String>,

    #[arg(long = "profile-cpu")]
    pub(crate) profile_cpu: Option<PathBuf>,

    #[arg(long = "profile-mem")]
    pub(crate) profile_mem: Option<PathBuf>,

    #[arg(long = "profile-block")]
    pub(crate) profile_block: Option<PathBuf>,

    #[arg(long = "profile-mutex")]
    pub(crate) profile_mutex: Option<PathBuf>,

    #[arg(long)]
    pub(crate) debug_query_variants: bool,

    #[arg(long)]
    pub(crate) explain: bool,

    #[arg(long = "debug-match", hide = true)]
    pub(crate) debug_match: bool,

    #[arg(long = "alias")]
    pub(crate) aliases: Vec<String>,

    #[arg(long)]
    pub(crate) bash: bool,

    #[arg(long)]
    pub(crate) zsh: bool,

    #[arg(long)]
    pub(crate) fish: bool,

    #[arg(long)]
    pub(crate) powershell: bool,
}

pub(crate) fn shell_script_kind(args: &Args) -> Result<Option<crate::shell::ShellKind>> {
    let selected = [args.bash, args.zsh, args.fish, args.powershell]
        .into_iter()
        .filter(|enabled| *enabled)
        .count();
    if selected > 1 {
        bail!("only one of --bash, --zsh, --fish, or --powershell can be used");
    }

    Ok(if args.bash {
        Some(crate::shell::ShellKind::Bash)
    } else if args.zsh {
        Some(crate::shell::ShellKind::Zsh)
    } else if args.fish {
        Some(crate::shell::ShellKind::Fish)
    } else if args.powershell {
        Some(crate::shell::ShellKind::PowerShell)
    } else {
        None
    })
}
