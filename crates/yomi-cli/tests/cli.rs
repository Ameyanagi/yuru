use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

const FIXTURE: &str = include_str!("fixtures/mixed_paths.txt");

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
fn cli_ja_query_ni_matches_seed_kanji_reading() {
    command()
        .args(["--lang", "ja", "--filter", "ni"])
        .write_stdin("tests/日本語.txt\ntests/日本人の.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("tests/日本語.txt"))
        .stdout(predicate::str::contains("tests/日本人の.txt"));
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
fn cli_read0_print0_tac_tail_no_sort() {
    command()
        .args([
            "--filter",
            "",
            "--read0",
            "--print0",
            "--tail",
            "2",
            "--tac",
            "--no-sort",
        ])
        .write_stdin("one\0two\0three\0")
        .assert()
        .success()
        .stdout(predicate::eq("three\0two\0"));
}

#[test]
fn cli_no_sort_preserves_input_order_without_rank_sorting() {
    command()
        .args(["--filter", "abc", "--no-sort", "--limit", "2"])
        .write_stdin("zzabc\nabc\nxxabc\n")
        .assert()
        .success()
        .stdout(predicate::eq("zzabc\nabc\n"));
}

#[test]
fn cli_empty_piped_stdin_does_not_fall_back_to_walker() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("match.txt"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args(["--filter", "match"])
        .write_stdin("")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}

#[test]
fn cli_uses_fzf_default_command_when_stdin_is_empty() {
    command()
        .env("FZF_DEFAULT_COMMAND", "printf 'alpha\\nbeta\\n'")
        .args(["--filter", "bet"])
        .assert()
        .success()
        .stdout(predicate::eq("beta\n"));
}

#[test]
fn cli_walks_files_when_explicit_walker_and_stdin_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("alpha.txt"), "").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested").join("beta.log"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args(["--filter", "beta", "--walker", "file,follow,hidden"])
        .assert()
        .success()
        .stdout(predicate::eq("nested/beta.log\n"));
}

#[test]
fn cli_walker_can_include_directories_and_skip_names() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("keep")).unwrap();
    fs::create_dir(dir.path().join("node_modules")).unwrap();
    fs::write(dir.path().join("node_modules").join("dep.js"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args([
            "--filter",
            "keep",
            "--walker",
            "file,dir",
            "--walker-skip",
            "node_modules",
        ])
        .assert()
        .success()
        .stdout(predicate::eq("keep\n"));
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

#[test]
fn cli_prints_bash_shell_integration_without_reading_fzf_opts() {
    command()
        .env("FZF_DEFAULT_OPTS", "--definitely-not-a-yomi-option")
        .args(["--bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("__yomi_ctrl_t__"))
        .stdout(predicate::str::contains("FZF_CTRL_T_COMMAND"))
        .stdout(predicate::str::contains("complete -D"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_prints_zsh_shell_integration() {
    command()
        .args(["--zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("zle -N __yomi_ctrl_r__"))
        .stdout(predicate::str::contains("bindkey '^T'"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_prints_fish_shell_integration() {
    command()
        .args(["--fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("function __yomi_ctrl_r__"))
        .stdout(predicate::str::contains("bind \\ct __yomi_ctrl_t__"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_rejects_multiple_shell_integration_flags() {
    command()
        .args(["--bash", "--zsh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "only one of --bash, --zsh, or --fish",
        ));
}

fn command() -> Command {
    let mut command = Command::cargo_bin("yomi").unwrap();
    command
        .env_remove("FZF_DEFAULT_OPTS")
        .env_remove("FZF_DEFAULT_OPTS_FILE")
        .env_remove("FZF_DEFAULT_COMMAND");
    command
}
