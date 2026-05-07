use anyhow::{bail, Context, Result};

use crate::{
    cli::{Args, FzfCompatArg, ZhPolyphoneArg, ZhScriptArg},
    options::has_unsupported_bindings,
};

pub(crate) fn enforce_fzf_compat(args: &Args) -> Result<()> {
    let _ = accepted_fzf_option_count(args);
    let mode = effective_fzf_compat(args)?;
    let ignored = ignored_fzf_options(args);
    if ignored.is_empty() || mode == FzfCompatArg::Ignore {
        return Ok(());
    }

    match mode {
        FzfCompatArg::Strict => {
            bail!(
                "unsupported fzf option(s): {}. Use --fzf-compat=warn or --fzf-compat=ignore to allow them",
                ignored.join(", ")
            );
        }
        FzfCompatArg::Warn => {
            for option in ignored {
                eprintln!("yuru: warning: ignoring unsupported fzf option {option}");
            }
        }
        FzfCompatArg::Ignore => {}
    }

    Ok(())
}

pub(crate) fn warn_reserved_zh_options(args: &Args) {
    if args.zh_polyphone == ZhPolyphoneArg::Phrase {
        eprintln!(
            "yuru: warning: --zh-polyphone=phrase is not implemented yet; using common polyphone expansion"
        );
    }
    if args.zh_script != ZhScriptArg::Auto {
        eprintln!("yuru: warning: --zh-script is reserved and currently has no effect");
    }
}

pub(crate) fn accepted_fzf_option_count(args: &Args) -> usize {
    macro_rules! count_bool {
        ($($field:ident),* $(,)?) => {
            0 $(+ usize::from(args.$field))*
        };
    }
    macro_rules! count_opt {
        ($($field:ident),* $(,)?) => {
            0 $(+ usize::from(args.$field.is_some()))*
        };
    }

    count_bool!(
        no_exact,
        extended_exact,
        no_extended,
        ignore_case,
        no_ignore_case,
        smart_case,
        no_sort,
        disabled,
        phony,
        enabled,
        no_phony,
        literal,
        no_literal,
        tac,
        no_tac,
        no_tail,
        read0,
        no_read0,
        sync,
        no_sync,
        print0,
        no_print0,
        ansi,
        no_ansi,
        print_query,
        no_print_query,
        select_1,
        no_select_1,
        exit_0,
        no_exit_0,
        no_multi,
        no_expect,
        no_preview,
        preview_auto,
        no_preview_border,
        no_height,
        no_popup,
        no_tmux,
        reverse,
        no_reverse,
        no_margin,
        no_padding,
        no_border,
        no_border_label,
        no_header,
        no_header_lines,
        header_first,
        no_header_first,
        no_header_border,
        no_header_lines_border,
        no_header_label,
        no_footer,
        no_footer_border,
        no_footer_label,
        no_color,
        no_256,
        bold,
        no_bold,
        black,
        no_black,
        cycle,
        no_cycle,
        highlight_line,
        no_highlight_line,
        no_wrap,
        wrap_word,
        no_wrap_word,
        multi_line,
        no_multi_line,
        raw,
        no_raw,
        track,
        no_track,
        no_id_nth,
        no_gap,
        no_gap_line,
        keep_right,
        no_keep_right,
        no_hscroll,
        hscroll,
        no_scrollbar,
        no_list_border,
        no_list_label,
        no_input,
        no_info_command,
        no_info,
        inline_info,
        no_inline_info,
        no_separator,
        filepath_word,
        no_filepath_word,
        no_input_border,
        no_input_label,
        no_listen,
        no_listen_unsafe,
        no_history,
        no_tty_default,
        force_tty_in,
        no_force_tty_in,
        no_winpty,
        no_mouse,
        no_unicode,
        unicode,
        ambidouble,
        no_ambidouble,
        clear,
        no_clear,
        man,
    ) + count_opt!(
        sort,
        tail,
        expect,
        toggle_sort,
        preview,
        preview_text_extensions,
        preview_window,
        preview_border,
        preview_label,
        preview_label_pos,
        preview_wrap_sign,
        height,
        min_height,
        popup,
        tmux,
        layout,
        margin,
        padding,
        border,
        border_label,
        border_label_pos,
        prompt,
        header,
        header_lines,
        header_border,
        header_lines_border,
        header_label,
        header_label_pos,
        footer,
        footer_border,
        footer_label,
        footer_label_pos,
        wrap,
        wrap_sign,
        id_nth,
        gap,
        gap_line,
        freeze_left,
        freeze_right,
        scroll_off,
        hscroll_off,
        jump_labels,
        gutter,
        gutter_raw,
        pointer,
        marker,
        marker_multi_line,
        ellipsis,
        tabstop,
        scrollbar,
        list_border,
        list_label,
        list_label_pos,
        info,
        info_command,
        separator,
        ghost,
        input_border,
        input_label,
        input_label_pos,
        with_shell,
        style,
        listen,
        listen_unsafe,
        history,
        history_size,
        tty_default,
        proxy_script,
        threads,
        bench,
        profile_cpu,
        profile_mem,
        profile_block,
        profile_mutex,
    ) + args.bind.len()
        + args.color.len()
}

fn effective_fzf_compat(args: &Args) -> Result<FzfCompatArg> {
    if let Some(mode) = args.fzf_compat {
        return Ok(mode);
    }

    match std::env::var("YURU_FZF_COMPAT") {
        Ok(value) => parse_fzf_compat_env(&value),
        Err(std::env::VarError::NotPresent) => Ok(FzfCompatArg::Warn),
        Err(error) => Err(error).context("failed to read YURU_FZF_COMPAT"),
    }
}

fn parse_fzf_compat_env(value: &str) -> Result<FzfCompatArg> {
    match value.trim() {
        "strict" => Ok(FzfCompatArg::Strict),
        "warn" => Ok(FzfCompatArg::Warn),
        "ignore" => Ok(FzfCompatArg::Ignore),
        other => bail!("unsupported YURU_FZF_COMPAT value: {other}"),
    }
}

fn ignored_fzf_options(args: &Args) -> Vec<&'static str> {
    let mut out = Vec::new();

    if has_unsupported_bindings(&args.bind) {
        out.push("--bind");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn accepted_fzf_options_are_counted() {
        let args = Args::parse_from([
            "yuru",
            "--extended-exact",
            "--no-exact",
            "--scheme",
            "path",
            "--bind",
            "ctrl-j:preview-down",
            "--preview",
            "cat {}",
            "--height",
            "40%",
            "--multi=3",
        ]);

        assert!(accepted_fzf_option_count(&args) > 0);
    }
}
