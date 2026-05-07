mod support;

use predicates::prelude::*;
use std::fs;
use support::command;

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
fn cli_preserves_invalid_utf8_bytes_on_default_output() {
    let output = command()
        .args(["--filter", "bad", "--no-sort"])
        .write_stdin(b"bad\xff\nother\n".as_slice())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"bad\xff\n");
}
#[test]
fn cli_preserves_invalid_utf8_bytes_with_read0_print0() {
    let output = command()
        .args(["--filter", "bad", "--read0", "--print0"])
        .write_stdin(b"bad\xff\0other\0".as_slice())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"bad\xff\0");
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
fn cli_header_lines_are_not_search_candidates() {
    command()
        .args(["--header-lines", "1", "--filter", "head"])
        .write_stdin("header\nalpha\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));

    command()
        .args(["--header-lines", "1", "--filter", "alpha"])
        .write_stdin("header\nalpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"));
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
fn cli_uses_yuru_default_command_when_stdin_is_empty() {
    command()
        .env("YURU_DEFAULT_COMMAND", "printf 'alpha\\nbeta\\n'")
        .args(["--filter", "alph"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"));
}
#[test]
fn cli_default_command_respects_read0_print0() {
    command()
        .env("YURU_DEFAULT_COMMAND", "printf 'one\\0two\\0'")
        .args(["--filter", "two", "--read0", "--print0"])
        .assert()
        .success()
        .stdout(predicate::eq("two\0"));
}
#[test]
fn cli_yuru_default_command_overrides_fzf_default_command() {
    command()
        .env("YURU_DEFAULT_COMMAND", "printf 'alpha\\n'")
        .env("FZF_DEFAULT_COMMAND", "printf 'beta\\n'")
        .args(["--filter", "alpha"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"));
}
#[test]
fn cli_reads_candidates_from_input_file_without_stdin_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("candidates.txt");
    fs::write(&input, "alpha\nbeta\n").unwrap();

    command()
        .args(["--filter", "beta", "--input", &input.to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::eq("beta\n"));
}
#[test]
fn cli_prints_version() {
    command()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("yuru "));
}
#[test]
fn cli_help_describes_non_prototype_tool_and_algo_aliases() {
    command()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("prototype").not())
        .stdout(predicate::str::contains(
            "A fast phonetic fuzzy finder for multilingual shell workflows",
        ))
        .stdout(predicate::str::contains(
            "greedy and fzf-v1 use Yuru's greedy scorer",
        ))
        .stdout(predicate::str::contains(
            "not byte-for-byte fzf algorithm implementations",
        ));
}
