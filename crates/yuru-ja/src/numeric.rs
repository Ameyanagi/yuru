use yuru_core::SourceSpan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NumericTokenizerInput {
    pub text: String,
    pub source_map: Vec<SourceSpan>,
}

pub(crate) fn numeric_source_digits(
    run: &str,
    span: SourceSpan,
) -> Option<Vec<(char, SourceSpan)>> {
    let mut digits = Vec::new();

    for (index, ch) in run
        .chars()
        .enumerate()
        .skip(span.start)
        .take(span.end.saturating_sub(span.start))
    {
        if let Some(digit) = digit_value(ch) {
            digits.push((
                char::from(b'0' + digit),
                SourceSpan {
                    start: index,
                    end: index + 1,
                },
            ));
        } else if !is_numeric_separator(ch) {
            return None;
        }
    }

    (!digits.is_empty()).then_some(digits)
}

pub(crate) fn numeric_context_tokenizer_input(run: &str) -> Option<NumericTokenizerInput> {
    if !has_convertible_digit_run(run) {
        return None;
    }

    let chars: Vec<char> = run.chars().collect();
    let mut text = String::with_capacity(run.len());
    let mut source_map = Vec::new();
    let mut changed = false;
    let mut index = 0usize;

    while index < chars.len() {
        if let Some(digit) = digit_value(chars[index]) {
            let start = index;
            let mut digits = vec![digit];
            index += 1;

            while index < chars.len() {
                if is_numeric_separator(chars[index]) {
                    index += 1;
                    continue;
                }
                if let Some(digit) = digit_value(chars[index]) {
                    digits.push(digit);
                    index += 1;
                    continue;
                }
                break;
            }

            let end = index;
            if let Some(numeral) = japanese_numeral_for_digits(&digits) {
                changed = true;
                let span = SourceSpan { start, end };
                text.push_str(&numeral);
                source_map.extend(numeral.chars().map(|_| span));
            } else {
                push_original_chars(&mut text, &mut source_map, &chars, start, end);
            }
            continue;
        }

        text.push(chars[index]);
        source_map.push(SourceSpan {
            start: index,
            end: index + 1,
        });
        index += 1;
    }

    changed.then_some(NumericTokenizerInput { text, source_map })
}

pub(crate) fn numeric_romaji_query(input: &str) -> Option<String> {
    let normalized = yuru_core::normalize::normalize(input);
    if !normalized.chars().any(|ch| ch.is_ascii_alphabetic())
        || !has_convertible_digit_run(&normalized)
    {
        return None;
    }

    let chars: Vec<char> = normalized.chars().collect();
    let mut out = String::with_capacity(normalized.len());
    let mut changed = false;
    let mut index = 0usize;

    while index < chars.len() {
        if let Some(digit) = digit_value(chars[index]) {
            let start = index;
            let mut digits = vec![digit];
            index += 1;

            while index < chars.len() {
                if is_numeric_separator(chars[index]) {
                    index += 1;
                    continue;
                }
                if let Some(digit) = digit_value(chars[index]) {
                    digits.push(digit);
                    index += 1;
                    continue;
                }
                break;
            }

            if let Some(romaji) = japanese_romaji_for_digits(&digits) {
                changed = true;
                out.push_str(&romaji);
            } else {
                chars[start..index].iter().for_each(|ch| out.push(*ch));
            }
            continue;
        }

        out.push(chars[index]);
        index += 1;
    }

    changed.then_some(out)
}

fn push_original_chars(
    text: &mut String,
    source_map: &mut Vec<SourceSpan>,
    chars: &[char],
    start: usize,
    end: usize,
) {
    for (offset, ch) in chars.iter().enumerate().take(end).skip(start) {
        text.push(*ch);
        source_map.push(SourceSpan {
            start: offset,
            end: offset + 1,
        });
    }
}

fn digit_value(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(ch as u8 - b'0'),
        '０'..='９' => Some((ch as u32 - '０' as u32) as u8),
        _ => None,
    }
}

fn is_numeric_separator(ch: char) -> bool {
    matches!(ch, ',' | '，' | '_')
}

fn has_convertible_digit_run(run: &str) -> bool {
    let mut chars = run.chars().peekable();

    while let Some(ch) = chars.next() {
        let Some(first) = digit_value(ch) else {
            continue;
        };

        let mut value = u32::from(first);
        let mut len = 1usize;

        while let Some(next) = chars.peek().copied() {
            if is_numeric_separator(next) {
                chars.next();
                continue;
            }
            let Some(digit) = digit_value(next) else {
                break;
            };
            chars.next();
            len += 1;
            value = value.saturating_mul(10).saturating_add(u32::from(digit));
        }

        if digit_run_is_convertible(first, len, value) {
            return true;
        }
    }

    false
}

fn japanese_numeral_for_digits(digits: &[u8]) -> Option<String> {
    let value = digits.iter().try_fold(0u32, |value, digit| {
        value.checked_mul(10)?.checked_add(u32::from(*digit))
    })?;
    if !digit_run_is_convertible(*digits.first()?, digits.len(), value) {
        return None;
    }

    Some(japanese_numeral_under_10000(value))
}

fn japanese_romaji_for_digits(digits: &[u8]) -> Option<String> {
    let value = digits.iter().try_fold(0u32, |value, digit| {
        value.checked_mul(10)?.checked_add(u32::from(*digit))
    })?;
    if !digit_run_is_convertible(*digits.first()?, digits.len(), value) {
        return None;
    }

    Some(japanese_romaji_under_10000(value))
}

fn digit_run_is_convertible(first: u8, len: usize, value: u32) -> bool {
    !(len > 1 && first == 0) && (1..=9999).contains(&value)
}

fn japanese_numeral_under_10000(value: u32) -> String {
    debug_assert!((1..=9999).contains(&value));

    let thousands = value / 1000;
    let hundreds = (value / 100) % 10;
    let tens = (value / 10) % 10;
    let ones = value % 10;
    let mut out = String::new();

    push_japanese_unit(&mut out, thousands, "千");
    push_japanese_unit(&mut out, hundreds, "百");
    push_japanese_unit(&mut out, tens, "十");
    if ones > 0 {
        out.push_str(japanese_digit(ones));
    }

    out
}

fn push_japanese_unit(out: &mut String, digit: u32, unit: &str) {
    match digit {
        0 => {}
        1 => out.push_str(unit),
        _ => {
            out.push_str(japanese_digit(digit));
            out.push_str(unit);
        }
    }
}

fn japanese_digit(digit: u32) -> &'static str {
    match digit {
        1 => "一",
        2 => "二",
        3 => "三",
        4 => "四",
        5 => "五",
        6 => "六",
        7 => "七",
        8 => "八",
        9 => "九",
        _ => "",
    }
}

fn japanese_romaji_under_10000(value: u32) -> String {
    debug_assert!((1..=9999).contains(&value));

    let thousands = value / 1000;
    let hundreds = (value / 100) % 10;
    let tens = (value / 10) % 10;
    let ones = value % 10;
    let mut out = String::new();

    push_romaji_unit(&mut out, thousands, "sen");
    push_romaji_unit(&mut out, hundreds, "hyaku");
    push_romaji_unit(&mut out, tens, "juu");
    if ones > 0 {
        out.push_str(japanese_digit_romaji(ones));
    }

    out
}

fn push_romaji_unit(out: &mut String, digit: u32, unit: &str) {
    match digit {
        0 => {}
        1 => out.push_str(unit),
        _ => {
            out.push_str(japanese_digit_romaji(digit));
            out.push_str(unit);
        }
    }
}

fn japanese_digit_romaji(digit: u32) -> &'static str {
    match digit {
        1 => "ichi",
        2 => "ni",
        3 => "san",
        4 => "yon",
        5 => "go",
        6 => "roku",
        7 => "nana",
        8 => "hachi",
        9 => "kyuu",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_numeric_runs_to_japanese_numerals() {
        let input = numeric_context_tokenizer_input("2025年8月").unwrap();

        assert_eq!(input.text, "二千二十五年八月");
        assert_eq!(input.source_map[0], SourceSpan { start: 0, end: 4 });
        assert_eq!(input.source_map[5], SourceSpan { start: 4, end: 5 });
        assert_eq!(input.source_map[6], SourceSpan { start: 5, end: 6 });
        assert_eq!(input.source_map[7], SourceSpan { start: 6, end: 7 });
    }

    #[test]
    fn supports_full_width_digits() {
        let input = numeric_context_tokenizer_input("８月").unwrap();

        assert_eq!(input.text, "八月");
        assert_eq!(input.source_map[0], SourceSpan { start: 0, end: 1 });
    }

    #[test]
    fn leaves_zero_and_large_numbers_unchanged() {
        let input = numeric_context_tokenizer_input("1年000001号10000年").unwrap();

        assert_eq!(input.text, "一年000001号10000年");
        assert_eq!(input.source_map[0], SourceSpan { start: 0, end: 1 });
        assert_eq!(input.source_map[2], SourceSpan { start: 2, end: 3 });
    }

    #[test]
    fn returns_none_when_there_is_no_convertible_number() {
        assert!(numeric_context_tokenizer_input("月").is_none());
        assert!(numeric_context_tokenizer_input("000001号").is_none());
    }

    #[test]
    fn expands_mixed_numeric_romaji_query() {
        assert_eq!(
            numeric_romaji_query("8gatsu"),
            Some("hachigatsu".to_string())
        );
        assert_eq!(
            numeric_romaji_query("2025nen8gatsu"),
            Some("nisennijuugonenhachigatsu".to_string())
        );
        assert_eq!(
            numeric_romaji_query("８gatsu"),
            Some("hachigatsu".to_string())
        );
    }

    #[test]
    fn keeps_pure_numeric_queries_literal() {
        assert_eq!(numeric_romaji_query("8"), None);
        assert_eq!(numeric_romaji_query("000001gou"), None);
    }
}
