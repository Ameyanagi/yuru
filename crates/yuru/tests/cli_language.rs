mod support;

use predicates::prelude::*;
use support::{command, FIXTURE};

#[test]
fn cli_plain_query_readme() {
    command()
        .args(["--lang", "plain", "--query", "read", "--limit", "1"])
        .write_stdin(FIXTURE)
        .assert()
        .success()
        .stdout(predicate::str::contains("README.md"));
}
#[test]
fn cli_ja_query_kamera_matches_katakana() {
    command()
        .args(["--lang", "ja", "--query", "kamera", "--limit", "3"])
        .write_stdin(FIXTURE)
        .assert()
        .success()
        .stdout(predicate::str::contains("カメラ.txt"));
}
#[test]
fn cli_ja_query_tokyo_matches_when_alias_exists() {
    command()
        .args([
            "--lang",
            "ja",
            "--query",
            "tokyo",
            "--limit",
            "3",
            "--alias",
            "tokyo=東京駅.txt",
        ])
        .write_stdin(FIXTURE)
        .assert()
        .success()
        .stdout(predicate::str::contains("東京駅.txt"));
}
#[test]
fn cli_ja_query_ni_matches_lindera_kanji_reading() {
    command()
        .args(["--lang", "ja", "--filter", "ni"])
        .write_stdin("tests/日本語.txt\ntests/日本人の.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("tests/日本語.txt"))
        .stdout(predicate::str::contains("tests/日本人の.txt"));
}
#[test]
fn cli_ja_query_zyu_matches_kunrei_romaji() {
    command()
        .args(["--lang", "ja", "--filter", "zyu"])
        .write_stdin("重要事項\nju.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("重要事項\n"));
}
#[test]
fn cli_ja_query_zi_matches_ji_kana() {
    command()
        .args(["--lang", "ja", "--filter", "zi"])
        .write_stdin("じ.txt\nジ.txt\nji.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("じ.txt"))
        .stdout(predicate::str::contains("ジ.txt"))
        .stdout(predicate::str::contains("ji.txt").not());
}
#[test]
fn cli_ja_query_nn_matches_nasal_kana() {
    command()
        .args(["--lang", "ja", "--filter", "nn"])
        .write_stdin("ん.txt\nn.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("ん.txt\n"));
}
#[test]
fn cli_ja_query_small_kana_ime_aliases_match_kana() {
    command()
        .args(["--lang", "ja", "--filter", "ltsu"])
        .write_stdin("っ.txt\nつ.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("っ.txt\n"));

    command()
        .args(["--lang", "ja", "--filter", "lyu"])
        .write_stdin("ゅ.txt\nゆ.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("ゅ.txt\n"));
}
#[test]
fn cli_dash_query_matches_japanese_prolonged_sound_mark_in_plain_mode() {
    command()
        .args(["--lang", "plain", "--filter", "-"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));
}
#[test]
fn cli_japanese_query_with_prolonged_sound_mark_does_not_panic() {
    command()
        .args(["--lang", "ja", "--filter", "ハッピー"])
        .write_stdin("2025年8月　ﾊｯﾋﾟｰｽﾏｲﾙ写真展示室ｺｰﾄﾞ.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("2025年8月　ﾊｯﾋﾟｰｽﾏｲﾙ写真展示室ｺｰﾄﾞ.pdf\n"));
}
#[test]
fn cli_ja_fuzzy_romaji_matches_japanese_filename() {
    command()
        .args(["--lang", "ja", "--filter", "hapsu"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));
}
#[test]
fn cli_ja_ime_romaji_matches_mixed_kana_and_kanji_filename() {
    command()
        .args(["--lang", "ja", "--filter", "happi-sumairushasinntennzi"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));
}
#[test]
fn cli_ja_ime_romaji_matches_contextual_date_readings() {
    command()
        .args(["--lang", "ja", "--filter", "gatu"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));

    command()
        .args(["--lang", "ja", "--filter", "nen", "--explain"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("source span: 4..5 \"年\""));
}
#[test]
fn cli_ja_native_kana_and_digits_match_numeric_date_context() {
    command()
        .args(["--lang", "ja", "--filter", "はち", "--explain"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("source span: 5..7 \"8月\""));

    command()
        .args(["--lang", "ja", "--filter", "8"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));

    command()
        .args(["--lang", "ja", "--filter", "8gatsu", "--explain"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("source span: 5..7 \"8月\""));

    command()
        .args(["--lang", "ja", "--filter", "2025nen8gatsu", "--explain"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("source span: 0..7 \"2025年8月\""));

    command()
        .args(["--lang", "ja", "--filter", "20258gatsu", "--explain"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("source span: 0..7 \"2025年8月\""));
}
#[test]
fn cli_lang_auto_selects_japanese_for_kana_candidates_without_japanese_locale() {
    command()
        .env("LC_ALL", "C")
        .args(["--lang", "auto", "--filter", "hapsu"])
        .write_stdin("2025年8月　ハッピースマイル写真展示室コード.pdf\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq(
            "2025年8月　ハッピースマイル写真展示室コード.pdf\n",
        ));
}
#[test]
fn cli_ko_queries_match_hangul_keys() {
    command()
        .args(["--lang", "ko", "--filter", "hangeul"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));

    command()
        .args(["--lang", "ko", "--filter", "ㅎㄱ"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));

    command()
        .args(["--lang", "ko", "--filter", "gksrmf"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));
}
#[test]
fn cli_lang_auto_selects_korean_for_korean_locale_and_hangul_candidates() {
    command()
        .env("LC_ALL", "ko_KR.UTF-8")
        .args(["--lang", "auto", "--filter", "hangeul"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));
}
#[test]
fn cli_lang_auto_selects_korean_for_hangul_jamo_query() {
    command()
        .env("LC_ALL", "C")
        .args(["--lang", "auto", "--filter", "ㅎㄱ"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));
}
#[test]
fn cli_zh_query_bjdx_matches_beijing_university() {
    command()
        .args(["--lang", "zh", "--query", "bjdx", "--limit", "3"])
        .write_stdin(FIXTURE)
        .assert()
        .success()
        .stdout(predicate::str::contains("北京大学.txt"));
}
#[test]
fn cli_lang_all_matches_mixed_language_inputs() {
    let mixed = "北京大学.txt\nカメラ.txt\n한글.txt\n";

    command()
        .args(["--lang", "all", "--filter", "bjdx"])
        .write_stdin(mixed)
        .assert()
        .success()
        .stdout(predicate::eq("北京大学.txt\n"));

    command()
        .args(["--lang", "all", "--filter", "kamera"])
        .write_stdin(mixed)
        .assert()
        .success()
        .stdout(predicate::eq("カメラ.txt\n"));

    command()
        .args(["--lang", "all", "--filter", "hangeul"])
        .write_stdin(mixed)
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));

    command()
        .args(["--lang", "all", "--filter", "ㅎㄱ"])
        .write_stdin(mixed)
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));

    command()
        .args(["--lang", "all", "--filter", "gksrmf"])
        .write_stdin(mixed)
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));
}
#[test]
fn cli_lang_auto_still_uses_one_backend_for_mixed_language_inputs() {
    command()
        .env("LC_ALL", "C")
        .args(["--lang", "auto", "--filter", "hangeul"])
        .write_stdin("北京大学.txt\nカメラ.txt\n한글.txt\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
#[test]
fn cli_lang_auto_selects_japanese_for_japanese_locale_and_han_candidates() {
    command()
        .env("LC_ALL", "ja_JP.UTF-8")
        .args(["--lang", "auto", "--filter", "ni"])
        .write_stdin("tests/日本語.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::eq("tests/日本語.txt\n"));
}
#[test]
fn cli_lang_auto_selects_chinese_for_chinese_locale_and_han_candidates() {
    command()
        .env("LC_ALL", "zh_CN.UTF-8")
        .args(["--lang", "auto", "--filter", "bjdx"])
        .write_stdin("北京大学.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("北京大学.txt\n"));
}
#[test]
fn cli_ja_reading_none_disables_lindera_kanji_reading() {
    command()
        .args(["--lang", "ja", "--ja-reading", "none", "--filter", "ni"])
        .write_stdin("tests/日本語.txt\nplan.md\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
#[test]
fn cli_zh_initials_can_be_disabled_for_exact_initial_query() {
    command()
        .args(["--lang", "zh", "--no-zh-initials", "--filter", "'bjdx"])
        .write_stdin("北京大学.txt\nnotes.txt\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
#[test]
fn cli_zh_polyphone_common_matches_alternate_reading() {
    command()
        .args([
            "--lang",
            "zh",
            "--zh-polyphone",
            "common",
            "--filter",
            "huanmei",
        ])
        .write_stdin("还没\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("还没\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_zh_polyphone_none_rejects_alternate_reading() {
    command()
        .args([
            "--lang",
            "zh",
            "--zh-polyphone",
            "none",
            "--filter",
            "huanmei",
        ])
        .write_stdin("还没\nnotes.txt\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
#[test]
fn cli_zh_polyphone_phrase_warns_and_uses_common() {
    command()
        .args([
            "--lang",
            "zh",
            "--zh-polyphone",
            "phrase",
            "--filter",
            "huanmei",
        ])
        .write_stdin("还没\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("还没\n"))
        .stderr(predicate::str::contains(
            "--zh-polyphone=phrase is not implemented",
        ));
}
#[test]
fn cli_zh_script_warns_reserved_when_non_auto() {
    command()
        .args(["--lang", "zh", "--zh-script", "hans", "--filter", "bjdx"])
        .write_stdin("北京大学.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("北京大学.txt\n"))
        .stderr(predicate::str::contains(
            "--zh-script is reserved and currently has no effect",
        ));
}
#[test]
fn cli_hides_unimplemented_chinese_options_from_help() {
    command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--zh-script").not())
        .stdout(predicate::str::contains("phrase").not());
}
#[test]
fn cli_caps_query_variants() {
    command()
        .args([
            "--lang",
            "ja",
            "--query",
            "oooooooo",
            "--max-query-variants",
            "4",
            "--debug-query-variants",
        ])
        .write_stdin(FIXTURE)
        .assert()
        .success()
        .stdout(predicate::str::contains("variant_count=4"));
}
