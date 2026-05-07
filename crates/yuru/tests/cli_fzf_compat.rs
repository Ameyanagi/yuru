mod support;

use predicates::prelude::*;
use support::command;

#[test]
fn cli_safe_fzf_default_opts_drop_unsupported_options() {
    command()
        .env(
            "FZF_DEFAULT_OPTS",
            "--preview 'cat {}' --definitely-not-a-yuru-option --prompt 'pick> '",
        )
        .args(["--filter", "alpha"])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_can_load_all_fzf_default_opts() {
    command()
        .env("FZF_DEFAULT_OPTS", "--preview 'cat {}'")
        .args(["--load-fzf-default-opts", "all", "--filter", "alpha"])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_warns_for_parsed_but_unsupported_fzf_options_by_default() {
    command()
        .args([
            "--filter",
            "alpha",
            "--preview",
            "cat {}",
            "--bind",
            "ctrl-y:execute-silent(echo {})",
        ])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::str::contains(
            "ignoring unsupported fzf option --bind",
        ));
}
#[test]
fn cli_accepts_preview_in_strict_mode() {
    command()
        .args([
            "--filter",
            "alpha",
            "--preview",
            "cat {}",
            "--fzf-compat",
            "strict",
        ])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_rejects_unsupported_fzf_options_in_strict_mode() {
    command()
        .args([
            "--filter",
            "alpha",
            "--bind",
            "ctrl-y:execute-silent(echo {})",
            "--fzf-compat",
            "strict",
        ])
        .write_stdin("alpha\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported fzf option(s): --bind",
        ));
}
#[test]
fn cli_accepts_preview_in_ignore_mode() {
    command()
        .args([
            "--filter",
            "alpha",
            "--preview",
            "cat {}",
            "--fzf-compat",
            "ignore",
        ])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_can_ignore_later_unsupported_fzf_options() {
    command()
        .args([
            "--filter",
            "alpha",
            "--fzf-compat",
            "ignore",
            "--preview",
            "cat {}",
        ])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_does_not_warn_for_supported_expect_and_header_options() {
    command()
        .args([
            "--filter", "alpha", "--expect", "ctrl-y", "--header", "Pick one",
        ])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_does_not_warn_for_supported_bind_subset() {
    command()
        .args(["--filter", "alpha", "--bind", "ctrl-x:abort,ctrl-y:accept"])
        .write_stdin("alpha\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"))
        .stderr(predicate::eq(""));
}
