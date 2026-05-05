use unicode_normalization::UnicodeNormalization;

pub fn normalize(text: &str) -> String {
    text.nfkc().flat_map(char::to_lowercase).collect()
}

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

pub fn contains_kana(text: &str) -> bool {
    text.chars()
        .any(|ch| ('ぁ'..='ゖ').contains(&ch) || ('ァ'..='ヶ').contains(&ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ascii_lowercase() {
        assert_eq!(normalize("README.MD"), "readme.md");
    }

    #[test]
    fn normalize_fullwidth_ascii_nfkc() {
        assert_eq!(normalize("ＡＢＣ１２３"), "abc123");
    }

    #[test]
    fn normalize_halfwidth_katakana() {
        assert_eq!(normalize("ｶﾒﾗ"), "カメラ");
    }

    #[test]
    fn katakana_to_hiragana_basic() {
        assert_eq!(katakana_to_hiragana("カメラ"), "かめら");
    }

    #[test]
    fn hiragana_to_katakana_basic() {
        assert_eq!(hiragana_to_katakana("しんじゅく"), "シンジュク");
    }
}
