use unicode_normalization::UnicodeNormalization;

/// Applies NFKC normalization, lowercasing, and dash-width folding.
pub fn normalize(text: &str) -> String {
    text.nfkc()
        .flat_map(char::to_lowercase)
        .map(fold_width_compatible_char)
        .collect()
}

/// Folds width-compatible dash and prolonged-sound variants to ASCII `-`.
pub fn fold_width_compatible_char(ch: char) -> char {
    match ch {
        '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{2212}' | '\u{30a0}' | '\u{30fc}' | '\u{fe58}' | '\u{fe63}' | '\u{ff0d}'
        | '\u{ff70}' => '-',
        _ => ch,
    }
}

/// Converts katakana characters in `text` to hiragana.
pub fn katakana_to_hiragana(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ('ァ'..='ヶ').contains(&ch) {
                char::from_u32(ch as u32 - 0x60).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Converts hiragana characters in `text` to katakana.
pub fn hiragana_to_katakana(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ('ぁ'..='ゖ').contains(&ch) {
                char::from_u32(ch as u32 + 0x60).unwrap_or(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Returns true when `text` contains hiragana or katakana.
pub fn contains_kana(text: &str) -> bool {
    text.chars()
        .any(|ch| ('ぁ'..='ゖ').contains(&ch) || ('ァ'..='ヶ').contains(&ch))
}

#[cfg(test)]
mod tests;
