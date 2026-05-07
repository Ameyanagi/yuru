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
fn normalize_folds_dash_and_prolonged_sound_width_variants() {
    assert_eq!(normalize("ハッピー-ｰ－―−゠"), "ハッピ-------");
}

#[test]
fn normalize_folds_fullwidth_space() {
    assert_eq!(normalize("2025年8月　PDF"), "2025年8月 pdf");
}

#[test]
fn katakana_to_hiragana_basic() {
    assert_eq!(katakana_to_hiragana("カメラ"), "かめら");
}

#[test]
fn hiragana_to_katakana_basic() {
    assert_eq!(hiragana_to_katakana("しんじゅく"), "シンジュク");
}
