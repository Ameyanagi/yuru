use anyhow::{bail, Context, Result};
use regex::Regex;

#[derive(Clone, Debug)]
pub struct FieldConfig {
    pub delimiter: Option<String>,
    pub nth: Option<String>,
    pub with_nth: Option<String>,
    pub accept_nth: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InputItem {
    pub original: String,
    pub display: String,
    pub search_text: String,
}

pub fn prepare_items(
    raw_items: Vec<String>,
    config: &FieldConfig,
    ansi: bool,
) -> Result<Vec<InputItem>> {
    raw_items
        .into_iter()
        .enumerate()
        .map(|(index, original)| {
            let searchable_original = if ansi {
                strip_ansi_codes(&original)
            } else {
                original.clone()
            };
            let display = if let Some(spec) = &config.with_nth {
                transform_line(
                    &searchable_original,
                    spec,
                    config.delimiter.as_deref(),
                    index,
                )
                .with_context(|| format!("invalid --with-nth expression: {spec}"))?
            } else {
                original.clone()
            };
            let search_base = if config.with_nth.is_some() {
                if ansi {
                    strip_ansi_codes(&display)
                } else {
                    display.clone()
                }
            } else {
                searchable_original
            };
            let search_text = if let Some(spec) = &config.nth {
                transform_line(&search_base, spec, config.delimiter.as_deref(), index)
                    .with_context(|| format!("invalid --nth expression: {spec}"))?
            } else {
                search_base
            };

            Ok(InputItem {
                original,
                display,
                search_text,
            })
        })
        .collect()
}

pub fn accept_output(item: &InputItem, config: &FieldConfig, ordinal: usize) -> Result<String> {
    if let Some(spec) = &config.accept_nth {
        transform_line(&item.original, spec, config.delimiter.as_deref(), ordinal)
            .with_context(|| format!("invalid --accept-nth expression: {spec}"))
    } else {
        Ok(item.original.clone())
    }
}

fn transform_line(
    line: &str,
    spec: &str,
    delimiter: Option<&str>,
    ordinal: usize,
) -> Result<String> {
    let split = split_fields(line, delimiter)?;
    if spec.contains('{') {
        render_template(spec, &split, ordinal)
    } else {
        Ok(select_fields(spec, &split))
    }
}

#[derive(Clone, Debug)]
struct SplitFields {
    fields: Vec<String>,
    joiner: String,
}

fn split_fields(line: &str, delimiter: Option<&str>) -> Result<SplitFields> {
    if let Some(delimiter) = delimiter {
        let regex = Regex::new(delimiter).context("invalid delimiter regex")?;
        Ok(SplitFields {
            fields: regex.split(line).map(str::to_string).collect(),
            joiner: delimiter.to_string(),
        })
    } else {
        Ok(SplitFields {
            fields: line.split_whitespace().map(str::to_string).collect(),
            joiner: " ".to_string(),
        })
    }
}

fn render_template(template: &str, split: &SplitFields, ordinal: usize) -> Result<String> {
    let mut out = String::new();
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('}') else {
            bail!("missing closing brace in field template");
        };
        let expr = &after_start[..end];
        if expr == "n" {
            out.push_str(&ordinal.to_string());
        } else {
            out.push_str(&select_fields(expr, split));
        }
        rest = &after_start[end + 1..];
    }

    out.push_str(rest);
    Ok(out)
}

fn select_fields(spec: &str, split: &SplitFields) -> String {
    let mut selected = Vec::new();
    for expr in spec
        .split(',')
        .map(str::trim)
        .filter(|expr| !expr.is_empty())
    {
        selected.extend(resolve_expr(expr, split));
    }
    selected.join(&split.joiner)
}

fn resolve_expr(expr: &str, split: &SplitFields) -> Vec<String> {
    if expr == ".." {
        return split.fields.clone();
    }

    if let Some((begin, end)) = expr.split_once("..") {
        let start = if begin.trim().is_empty() {
            Some(0)
        } else {
            resolve_index(begin.trim(), split.fields.len())
        };
        let end = if end.trim().is_empty() {
            split.fields.len().checked_sub(1)
        } else {
            resolve_index(end.trim(), split.fields.len())
        };

        if let (Some(start), Some(end)) = (start, end) {
            if start <= end {
                return split.fields[start..=end].to_vec();
            }
        }
        return Vec::new();
    }

    resolve_index(expr, split.fields.len())
        .and_then(|index| split.fields.get(index).cloned())
        .into_iter()
        .collect()
}

fn resolve_index(raw: &str, len: usize) -> Option<usize> {
    let index: isize = raw.parse().ok()?;
    if index == 0 {
        return None;
    }

    let resolved = if index > 0 {
        index - 1
    } else {
        len as isize + index
    };

    (resolved >= 0 && resolved < len as isize).then_some(resolved as usize)
}

pub fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }

        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nth_selects_positive_and_negative_fields() {
        let split = split_fields("foo bar baz", None).unwrap();
        assert_eq!(select_fields("2,-1", &split), "bar baz");
        assert_eq!(select_fields("2..", &split), "bar baz");
        assert_eq!(select_fields("..-2", &split), "foo bar");
    }

    #[test]
    fn template_renders_fields_and_index() {
        assert_eq!(
            transform_line("foo,bar,baz", "{n}:{3}:{1}", Some(","), 7).unwrap(),
            "7:baz:foo"
        );
    }

    #[test]
    fn ansi_codes_are_stripped() {
        assert_eq!(strip_ansi_codes("\u{1b}[31mred\u{1b}[0m"), "red");
    }
}
