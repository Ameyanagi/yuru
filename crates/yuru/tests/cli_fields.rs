mod support;

use predicates::prelude::*;
use support::command;

#[test]
fn cli_explain_reports_plain_match_key() {
    command()
        .args(["--filter", "read", "--explain", "--limit", "1"])
        .write_stdin("README.md\nCargo.toml\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("README.md\n  score:"))
        .stdout(predicate::str::contains("matched key: Normalized"))
        .stdout(predicate::str::contains("matched text: read"))
        .stdout(predicate::str::contains("source span: 0..4"));
}
#[test]
fn cli_explain_reports_japanese_romaji_source_span() {
    command()
        .args([
            "--lang",
            "ja",
            "--filter",
            "ni",
            "--explain",
            "--limit",
            "1",
        ])
        .write_stdin("tests/日本語.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("tests/日本語.txt"))
        .stdout(predicate::str::contains("matched key: RomajiReading"))
        .stdout(predicate::str::contains("source span:"))
        .stdout(predicate::str::contains("\"日本"));
}
#[test]
fn cli_explain_reports_chinese_initial_source_span() {
    command()
        .args(["--lang", "zh", "--filter", "bjdx", "--explain"])
        .write_stdin("北京大学.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("北京大学.txt"))
        .stdout(predicate::str::contains("matched key: PinyinInitials"))
        .stdout(predicate::str::contains("source span: 0..4 \"北京大学\""));
}
#[test]
fn cli_explain_reports_korean_romanized_source_span() {
    command()
        .args(["--lang", "ko", "--filter", "hg", "--explain"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("한글.txt"))
        .stdout(predicate::str::contains("matched key: KoreanRomanized"))
        .stdout(predicate::str::contains("source span: 0..2 \"한글\""));
}
#[test]
fn cli_debug_match_is_hidden_alias_for_explain() {
    command()
        .args([
            "--lang",
            "ja",
            "--filter",
            "tokyo",
            "--alias",
            "tokyo=東京駅.txt",
            "--debug-match",
        ])
        .write_stdin("東京駅.txt\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("matched key: LearnedAlias"))
        .stdout(predicate::str::contains("source span: n/a"));
}
#[test]
fn cli_explain_preserves_no_match_exit_status() {
    command()
        .args(["--filter", "missing", "--explain"])
        .write_stdin("alpha\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
#[test]
fn cli_explain_rejects_print0() {
    command()
        .args(["--filter", "alpha", "--explain", "--print0"])
        .write_stdin("alpha\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--explain cannot be combined with --print0",
        ));
}
#[test]
fn cli_filter_exact_does_not_match_subsequence() {
    command()
        .args(["--filter", "abc", "--exact"])
        .write_stdin("a_b_c\nabc\n")
        .assert()
        .success()
        .stdout(predicate::eq("abc\n"));
}
#[test]
fn cli_algo_nucleo_filters_candidates() {
    command()
        .args(["--algo", "nucleo", "--filter", "rdme"])
        .write_stdin("README.md\nCargo.toml\n")
        .assert()
        .success()
        .stdout(predicate::eq("README.md\n"));
}
#[test]
fn cli_algo_fzf_v2_uses_quality_matcher_path() {
    command()
        .args(["--algo", "fzf-v2", "--filter", "rdme"])
        .write_stdin("README.md\nCargo.toml\n")
        .assert()
        .success()
        .stdout(predicate::eq("README.md\n"));
}
#[test]
fn cli_extended_negation_filters_results() {
    command()
        .args(["--filter", "src !test"])
        .write_stdin("src/main.rs\nsrc/test.rs\n")
        .assert()
        .success()
        .stdout(predicate::eq("src/main.rs\n"));
}
#[test]
fn cli_field_scope_and_accept_transform() {
    command()
        .args([
            "--filter",
            "bar",
            "--delimiter",
            ",",
            "--nth",
            "2",
            "--accept-nth",
            "{3}:{1}",
        ])
        .write_stdin("foo,bar,baz\nzip,zap,zop\n")
        .assert()
        .success()
        .stdout(predicate::eq("baz:foo\n"));
}
#[test]
fn cli_tiebreak_length_is_default_for_equal_scores() {
    command()
        .args(["--filter", "", "--disabled"])
        .write_stdin("aaaa\naa\n")
        .assert()
        .success()
        .stdout(predicate::eq("aa\naaaa\n"));
}
#[test]
fn cli_tiebreak_index_preserves_input_order_for_equal_scores() {
    command()
        .args(["--filter", "", "--disabled", "--tiebreak", "index"])
        .write_stdin("aaaa\naa\n")
        .assert()
        .success()
        .stdout(predicate::eq("aaaa\naa\n"));
}
#[test]
fn cli_scheme_path_prefers_match_in_basename() {
    command()
        .args(["--filter", "foo", "--disabled", "--scheme", "path"])
        .write_stdin("foo/file.txt\nsrc/foo.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("src/foo.txt\nfoo/file.txt\n"));
}
