use std::io::IsTerminal;

use anyhow::{bail, Result};
use yuru_core::{MatcherAlgo, SearchConfig, Tiebreak};

use crate::{
    cli::{
        AlgoArg, Args, PreviewImageProtocolArg, SchemeArg, DEFAULT_INTERACTIVE_LIMIT,
        DEFAULT_PREVIEW_TEXT_EXTENSIONS,
    },
    fields::FieldConfig,
};

#[derive(Clone, Debug)]
pub(crate) struct RunOptions {
    pub(crate) query: String,
    pub(crate) interactive: bool,
    pub(crate) field_config: FieldConfig,
    pub(crate) search_config: SearchConfig,
}

impl RunOptions {
    pub(crate) fn from_args(args: &Args) -> Result<Self> {
        let query = effective_query(args);
        let interactive = should_run_interactive(args);
        let limit = args
            .limit
            .unwrap_or_else(|| default_limit(args, interactive));
        let field_config = FieldConfig {
            delimiter: args.delimiter.clone(),
            nth: args.nth.clone(),
            with_nth: args.with_nth.clone(),
            accept_nth: args.accept_nth.clone(),
        };
        let search_config = SearchConfig {
            max_query_variants: args.max_query_variants,
            max_search_keys_per_candidate: args.max_keys_per_candidate,
            max_total_key_bytes_per_candidate: args.max_total_key_bytes_per_candidate,
            limit,
            top_b_for_quality_score: args.top_b,
            exact: exact_enabled(args),
            extended: extended_enabled(args),
            case_sensitive: case_sensitive(&query, args),
            disabled: disabled_enabled(args),
            no_sort: no_sort_enabled(args),
            normalize: normalize_enabled(args),
            matcher_algo: matcher_algo(args.algo),
            tiebreaks: parse_tiebreaks(args)?,
        };

        Ok(Self {
            query,
            interactive,
            field_config,
            search_config,
        })
    }
}

pub(crate) fn effective_query(args: &Args) -> String {
    args.filter
        .as_ref()
        .or(args.query.as_ref())
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn case_sensitive(query: &str, args: &Args) -> bool {
    if args.ignore_case {
        return false;
    }
    if args.no_ignore_case {
        return true;
    }
    args.smart_case && query.chars().any(char::is_uppercase)
}

pub(crate) fn matcher_algo(value: AlgoArg) -> MatcherAlgo {
    match value {
        AlgoArg::Greedy => MatcherAlgo::Greedy,
        AlgoArg::FzfV1 => MatcherAlgo::FzfV1,
        AlgoArg::FzfV2 => MatcherAlgo::FzfV2,
        AlgoArg::Nucleo => MatcherAlgo::Nucleo,
    }
}

pub(crate) fn should_run_interactive(args: &Args) -> bool {
    should_run_interactive_with_tty(
        args,
        std::io::stderr().is_terminal() || env_flag_enabled("YURU_FORCE_INTERACTIVE"),
    )
}

pub(crate) fn should_run_interactive_with_tty(args: &Args, ui_tty_available: bool) -> bool {
    args.filter.is_none() && !args.debug_query_variants && !explain_mode(args) && ui_tty_available
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn explain_mode(args: &Args) -> bool {
    args.explain || args.debug_match
}

pub(crate) fn default_limit(args: &Args, interactive: bool) -> usize {
    if args.filter.is_some() {
        usize::MAX
    } else if interactive {
        DEFAULT_INTERACTIVE_LIMIT
    } else {
        10
    }
}

pub(crate) fn exact_enabled(args: &Args) -> bool {
    (args.exact || args.extended_exact) && !args.no_exact
}

pub(crate) fn extended_enabled(args: &Args) -> bool {
    (args.extended || args.extended_exact) && !args.no_extended
}

pub(crate) fn disabled_enabled(args: &Args) -> bool {
    (args.disabled || args.phony) && !(args.enabled || args.no_phony)
}

pub(crate) fn no_sort_enabled(args: &Args) -> bool {
    args.no_sort && args.sort.is_none()
}

pub(crate) fn normalize_enabled(args: &Args) -> bool {
    !args.literal || args.no_literal
}

pub(crate) fn tac_enabled(args: &Args) -> bool {
    args.tac && !args.no_tac
}

pub(crate) fn tail_count(args: &Args) -> Option<usize> {
    (!args.no_tail).then_some(args.tail).flatten()
}

pub(crate) fn read0_enabled(args: &Args) -> bool {
    args.read0 && !args.no_read0
}

pub(crate) fn sync_enabled(args: &Args) -> bool {
    args.sync && !args.no_sync
}

pub(crate) fn print0_enabled(args: &Args) -> bool {
    args.print0 && !args.no_print0
}

pub(crate) fn ansi_enabled(args: &Args) -> bool {
    args.ansi && !args.no_ansi
}

pub(crate) fn print_query_enabled(args: &Args) -> bool {
    args.print_query && !args.no_print_query
}

pub(crate) fn select_1_enabled(args: &Args) -> bool {
    args.select_1 && !args.no_select_1
}

pub(crate) fn exit_0_enabled(args: &Args) -> bool {
    args.exit_0 && !args.no_exit_0
}

pub(crate) fn multi_enabled(args: &Args) -> bool {
    args.multi.is_some() && !args.no_multi
}

pub(crate) fn multi_limit(args: &Args) -> Option<usize> {
    args.multi.flatten()
}

pub(crate) fn cycle_enabled(args: &Args) -> bool {
    args.cycle && !args.no_cycle
}

pub(crate) fn expect_arg(args: &Args) -> Option<&str> {
    (!args.no_expect)
        .then_some(args.expect.as_deref())
        .flatten()
}

pub(crate) fn preview_command(args: &Args) -> Option<yuru_tui::PreviewCommand> {
    if args.no_preview {
        return None;
    }
    if let Some(command) = &args.preview {
        return Some(yuru_tui::PreviewCommand::Shell(command.clone()));
    }
    args.preview_auto
        .then(|| yuru_tui::PreviewCommand::Builtin {
            text_extensions: preview_text_extensions(args),
        })
}

pub(crate) fn preview_text_extensions(args: &Args) -> Vec<String> {
    args.preview_text_extensions
        .as_deref()
        .map(parse_preview_text_extensions)
        .unwrap_or_else(default_preview_text_extensions)
}

pub(crate) fn parse_preview_text_extensions(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn default_preview_text_extensions() -> Vec<String> {
    DEFAULT_PREVIEW_TEXT_EXTENSIONS
        .iter()
        .map(|extension| (*extension).to_string())
        .collect()
}

pub(crate) fn preview_image_protocol(args: &Args) -> Option<yuru_tui::ImagePreviewProtocol> {
    match args.preview_image_protocol {
        PreviewImageProtocolArg::None => None,
        PreviewImageProtocolArg::Auto => Some(yuru_tui::ImagePreviewProtocol::Auto),
        PreviewImageProtocolArg::Halfblocks => Some(yuru_tui::ImagePreviewProtocol::Halfblocks),
        PreviewImageProtocolArg::Sixel => Some(yuru_tui::ImagePreviewProtocol::Sixel),
        PreviewImageProtocolArg::Kitty => Some(yuru_tui::ImagePreviewProtocol::Kitty),
        PreviewImageProtocolArg::Iterm2 => Some(yuru_tui::ImagePreviewProtocol::Iterm2),
    }
}

pub(crate) fn header_lines_count(args: &Args) -> usize {
    (!args.no_header_lines)
        .then_some(args.header_lines)
        .flatten()
        .unwrap_or(0)
}

pub(crate) fn split_header_lines(
    mut records: Vec<crate::fields::InputRecord>,
    count: usize,
) -> (
    Vec<crate::fields::InputRecord>,
    Vec<crate::fields::InputRecord>,
) {
    let split_at = count.min(records.len());
    let candidates = records.split_off(split_at);
    (records, candidates)
}

pub(crate) fn header_text(
    args: &Args,
    header_records: &[crate::fields::InputRecord],
) -> Option<String> {
    let mut lines = Vec::new();
    if !args.no_header {
        if let Some(header) = &args.header {
            lines.push(header.clone());
        }
    }
    if !args.no_header_lines {
        lines.extend(header_records.iter().map(|record| record.display.clone()));
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub(crate) fn footer_text(args: &Args) -> Option<String> {
    (!args.no_footer).then_some(args.footer.clone()).flatten()
}

pub(crate) fn parse_tui_height(args: &Args) -> Option<usize> {
    if args.no_height {
        return None;
    }
    args.height
        .as_deref()
        .and_then(|height| height.parse().ok())
        .filter(|height| *height > 0)
}

pub(crate) fn parse_tui_layout(args: &Args) -> Result<yuru_tui::TuiLayout> {
    if args.reverse {
        return Ok(yuru_tui::TuiLayout::Reverse);
    }

    match args.layout.as_deref().unwrap_or("default") {
        "default" => Ok(yuru_tui::TuiLayout::Default),
        "reverse" => Ok(yuru_tui::TuiLayout::Reverse),
        "reverse-list" => Ok(yuru_tui::TuiLayout::ReverseList),
        other => bail!("unsupported --layout value: {other}"),
    }
}

pub(crate) fn parse_tui_style(raw: &[Option<String>]) -> yuru_tui::TuiStyle {
    let mut style = yuru_tui::TuiStyle::default();
    for color_set in raw.iter().flatten() {
        for entry in color_set.split(',') {
            let Some((name, value)) = entry.split_once(':') else {
                continue;
            };
            let Some(color) = parse_hex_color(value) else {
                continue;
            };
            match name {
                "pointer" => style.pointer = Some(color),
                "hl" => style.highlight = Some(color),
                "hl+" => style.highlight_selected = Some(color),
                "fg+" => style.selected_fg = Some(color),
                "bg+" => style.selected_bg = Some(color),
                _ => {}
            }
        }
    }
    style
}

pub(crate) fn highlight_line_enabled(args: &Args) -> bool {
    !args.no_highlight_line
}

pub(crate) fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or_default().to_string()
}

fn parse_hex_color(value: &str) -> Option<yuru_tui::TuiRgb> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(yuru_tui::TuiRgb { r, g, b })
}

pub(crate) fn parse_expect_keys(raw: Option<&str>) -> Vec<String> {
    raw.into_iter()
        .flat_map(|keys| keys.split(','))
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| key.to_ascii_lowercase())
        .collect()
}

pub(crate) fn parse_bindings(raw: &[String]) -> Vec<yuru_tui::KeyBinding> {
    raw.iter()
        .flat_map(|bindings| bindings.split(','))
        .filter_map(parse_supported_binding)
        .collect()
}

pub(crate) fn parse_supported_binding(raw: &str) -> Option<yuru_tui::KeyBinding> {
    let (key, action) = raw.split_once(':')?;
    let action = match action.trim() {
        "accept" => yuru_tui::BindingAction::Accept,
        "abort" => yuru_tui::BindingAction::Abort,
        "clear-query" | "clear" | "unix-line-discard" => yuru_tui::BindingAction::ClearQuery,
        "up" | "previous" => yuru_tui::BindingAction::MoveSelectionUp,
        "down" | "next" => yuru_tui::BindingAction::MoveSelectionDown,
        "first" | "top" => yuru_tui::BindingAction::MoveSelectionFirst,
        "last" | "bottom" => yuru_tui::BindingAction::MoveSelectionLast,
        "page-up" => yuru_tui::BindingAction::PageUp,
        "page-down" => yuru_tui::BindingAction::PageDown,
        "toggle" => yuru_tui::BindingAction::ToggleMark,
        "toggle+down" => yuru_tui::BindingAction::ToggleMarkAndDown,
        "toggle+up" => yuru_tui::BindingAction::ToggleMarkAndUp,
        "beginning-of-line" => yuru_tui::BindingAction::MoveCursorStart,
        "end-of-line" => yuru_tui::BindingAction::MoveCursorEnd,
        "backward-char" => yuru_tui::BindingAction::MoveCursorLeft,
        "forward-char" => yuru_tui::BindingAction::MoveCursorRight,
        "backward-delete-char" => yuru_tui::BindingAction::Backspace,
        "delete-char" => yuru_tui::BindingAction::Delete,
        "preview-up" => yuru_tui::BindingAction::PreviewUp,
        "preview-down" => yuru_tui::BindingAction::PreviewDown,
        "preview-page-up" => yuru_tui::BindingAction::PreviewPageUp,
        "preview-page-down" => yuru_tui::BindingAction::PreviewPageDown,
        "preview-top" => yuru_tui::BindingAction::PreviewTop,
        "preview-bottom" => yuru_tui::BindingAction::PreviewBottom,
        _ => return None,
    };

    Some(yuru_tui::KeyBinding {
        key: normalize_binding_key(key),
        action,
    })
}

pub(crate) fn normalize_binding_key(key: &str) -> String {
    match key.trim().to_ascii_lowercase().as_str() {
        "btab" => "shift-tab".to_string(),
        "pgup" => "page-up".to_string(),
        "pgdn" => "page-down".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn has_unsupported_bindings(raw: &[String]) -> bool {
    raw.iter()
        .flat_map(|bindings| bindings.split(','))
        .map(str::trim)
        .filter(|binding| !binding.is_empty())
        .any(|binding| parse_supported_binding(binding).is_none())
}

pub(crate) fn parse_tiebreaks(args: &Args) -> Result<Vec<Tiebreak>> {
    let raw = match args.scheme {
        SchemeArg::Default => args.tiebreak.as_str(),
        SchemeArg::Path if args.tiebreak == "length" => "pathname,length",
        SchemeArg::Path => args.tiebreak.as_str(),
        SchemeArg::History if args.tiebreak == "length" => "index",
        SchemeArg::History => args.tiebreak.as_str(),
    };

    let mut out = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let criterion = match part {
            "length" => Tiebreak::Length,
            "chunk" => Tiebreak::Chunk,
            "pathname" => Tiebreak::Pathname,
            "begin" => Tiebreak::Begin,
            "end" => Tiebreak::End,
            "index" => Tiebreak::Index,
            other => bail!("unknown --tiebreak criterion: {other}"),
        };
        if out.contains(&criterion) {
            bail!("duplicate --tiebreak criterion: {part}");
        }
        if out.contains(&Tiebreak::Index) {
            bail!("--tiebreak=index is only allowed at the end");
        }
        out.push(criterion);
    }

    if out.is_empty() {
        out.push(Tiebreak::Length);
    }
    if !out.contains(&Tiebreak::Index) {
        out.push(Tiebreak::Index);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn filter_and_debug_modes_stay_non_interactive() {
        let filter_args = Args::parse_from(["yuru", "--filter", "abc"]);
        let debug_args = Args::parse_from(["yuru", "--debug-query-variants"]);

        assert!(!should_run_interactive_with_tty(&filter_args, true));
        assert!(!should_run_interactive_with_tty(&debug_args, true));
        assert!(!should_run_interactive_with_tty(
            &Args::parse_from(["yuru"]),
            false
        ));
    }

    #[test]
    fn interactive_mode_uses_larger_default_limit() {
        let interactive_args = Args::parse_from(["yuru"]);
        let batch_args = Args::parse_from(["yuru"]);
        let filter_args = Args::parse_from(["yuru", "--filter", "abc"]);

        assert_eq!(
            default_limit(&interactive_args, true),
            DEFAULT_INTERACTIVE_LIMIT
        );
        assert_eq!(default_limit(&batch_args, false), 10);
        assert_eq!(default_limit(&filter_args, false), usize::MAX);
    }

    #[test]
    fn preview_scroll_bind_actions_are_supported() {
        let binding = parse_supported_binding("ctrl-j:preview-down").unwrap();

        assert_eq!(binding.key, "ctrl-j");
        assert_eq!(binding.action, yuru_tui::BindingAction::PreviewDown);
        assert!(!has_unsupported_bindings(&[
            "ctrl-k:preview-up,ctrl-j:preview-down".to_string(),
            "ctrl-b:preview-page-up,ctrl-f:preview-page-down".to_string(),
            "home:preview-top,end:preview-bottom".to_string(),
        ]));
    }

    #[test]
    fn parse_tui_style_supports_selected_row_colors() {
        let style = parse_tui_style(&[Some(
            "pointer:#ff0000,hl:#00ff00,hl+:#00aa00,fg+:#ddeeff,bg+:#112233".to_string(),
        )]);

        assert_eq!(
            style.selected_fg,
            Some(yuru_tui::TuiRgb {
                r: 0xdd,
                g: 0xee,
                b: 0xff,
            })
        );
        assert_eq!(
            style.selected_bg,
            Some(yuru_tui::TuiRgb {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            })
        );
    }
}
