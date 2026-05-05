use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::MAIN_SEPARATOR;
#[cfg(unix)]
use std::process::Command as StdCommand;

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
fn cli_reads_default_language_from_yuru_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    fs::write(&config, "--lang ja\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .args(["--filter", "ni"])
        .write_stdin("tests/日本語.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::eq("tests/日本語.txt\n"));
}

#[test]
fn cli_args_override_yuru_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    fs::write(&config, "--lang ja\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .args(["--lang", "plain", "--filter", "ni"])
        .write_stdin("tests/日本語.txt\nplain-ni.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("plain-ni.txt\n"));
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
fn cli_uses_yuru_default_command_when_stdin_is_empty() {
    command()
        .env("YURU_DEFAULT_COMMAND", "printf 'alpha\\nbeta\\n'")
        .args(["--filter", "alph"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"));
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
        .stdout(predicate::eq(format!("nested{MAIN_SEPARATOR}beta.log\n")));
}

#[test]
fn cli_explicit_walker_ignores_invalid_fzf_default_command() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("alpha.txt"), "").unwrap();

    command()
        .current_dir(dir.path())
        .env(
            "FZF_DEFAULT_COMMAND",
            "fdfind --definitely-missing-yuru-test",
        )
        .args(["--filter", "alpha", "--walker", "file,follow,hidden"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha.txt\n"));
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

#[cfg(unix)]
#[test]
fn cli_walker_skips_broken_symlinks_when_following_links() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".config")).unwrap();
    std::os::unix::fs::symlink("missing", dir.path().join(".config").join("starship")).unwrap();
    fs::write(dir.path().join("alpha.txt"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args(["--filter", "alpha", "--walker", "file,follow,hidden"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha.txt\n"));
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
        .env("FZF_DEFAULT_OPTS", "--definitely-not-a-yuru-option")
        .args(["--bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("__yuru_ctrl_t__"))
        .stdout(predicate::str::contains("FZF_CTRL_T_COMMAND"))
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("__yuru_setup_completion__"))
        .stdout(predicate::str::contains("complete -D"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_prints_zsh_shell_integration() {
    command()
        .args(["--zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("zle -N __yuru_ctrl_r__"))
        .stdout(predicate::str::contains("__yuru_default_completion_widget"))
        .stdout(predicate::str::contains("bindkey -M emacs '^T'"))
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_prints_fish_shell_integration() {
    command()
        .args(["--fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("function __yuru_ctrl_r__"))
        .stdout(predicate::str::contains(
            "function __yuru_completion_trigger__",
        ))
        .stdout(predicate::str::contains("bind \\ct __yuru_ctrl_t__"))
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("**<TAB>"));
}

#[test]
fn cli_prints_powershell_shell_integration() {
    command()
        .args(["--powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set-PSReadLineKeyHandler"))
        .stdout(predicate::str::contains("Invoke-YuruCtrlT"))
        .stdout(predicate::str::contains("Invoke-YuruWithItems"))
        .stdout(predicate::str::contains("Get-YuruCompletionTrigger"))
        .stdout(predicate::str::contains("**<Tab>"));
}

#[cfg(unix)]
#[test]
fn bash_completion_joins_selected_paths_for_starstar_trigger() {
    let dir = tempfile::tempdir().unwrap();
    let script = write_shell_script(dir.path(), "yuru.bash", "--bash");
    let fake = write_fake_yuru(
        dir.path(),
        "fake-yuru",
        "printf 'src/main.rs\\nsrc/lib.rs\\n'\n",
    );

    let output = StdCommand::new("bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            r#"source "$YURU_SCRIPT"
COMP_WORDS=(vim 'src/**')
COMP_CWORD=1
__yuru_completion__
complete -p vim >/dev/null
printf '%s\n' "${COMPREPLY[0]}""#,
        ])
        .env("YURU_SCRIPT", &script)
        .env("YURU_BIN", &fake)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "src/main.rs src/lib.rs\n"
    );
}

#[cfg(unix)]
#[test]
fn bash_ctrl_r_passes_current_line_as_initial_query() {
    let dir = tempfile::tempdir().unwrap();
    let script = write_shell_script(dir.path(), "yuru.bash", "--bash");
    let fake = write_fake_yuru(
        dir.path(),
        "fake-yuru",
        "printf '%s\\n' \"$@\" > \"$YURU_FAKE_ARGS\"\nprintf 'git status\\n'\n",
    );
    let args_file = dir.path().join("args.txt");

    let output = StdCommand::new("bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            r#"source "$YURU_SCRIPT"
READLINE_LINE=git
READLINE_POINT=3
__yuru_ctrl_r__
printf '%s\n' "$READLINE_LINE"
cat "$YURU_FAKE_ARGS""#,
        ])
        .env("YURU_SCRIPT", &script)
        .env("YURU_BIN", &fake)
        .env("YURU_FAKE_ARGS", &args_file)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("git status\n"));
    assert!(stdout.contains("--query\ngit\n"), "stdout={stdout}");
    assert!(stdout.contains("--input\n"), "stdout={stdout}");
}

#[cfg(unix)]
#[test]
fn zsh_completion_replaces_starstar_token_and_keeps_prefix() {
    if StdCommand::new("zsh").arg("--version").output().is_err() {
        eprintln!("skipping zsh completion smoke because zsh is not installed");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let script = write_shell_script(dir.path(), "yuru.zsh", "--zsh");
    let fake = write_fake_yuru(
        dir.path(),
        "fake-yuru",
        "printf 'src/main.rs\\nsrc/lib.rs\\n'\n",
    );

    let output = StdCommand::new("zsh")
        .args([
            "-fc",
            r#"source "$YURU_SCRIPT"
YURU_BIN="$YURU_FAKE"
LBUFFER="vim src/**"
__yuru_completion__ 2>/dev/null
print -r -- "$LBUFFER""#,
        ])
        .env("YURU_SCRIPT", &script)
        .env("YURU_FAKE", &fake)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "vim src/main.rs src/lib.rs \n"
    );
}

#[cfg(unix)]
#[test]
fn zsh_ctrl_t_uses_input_file_for_command_candidates() {
    if StdCommand::new("zsh").arg("--version").output().is_err() {
        eprintln!("skipping zsh ctrl-t smoke because zsh is not installed");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let script = write_shell_script(dir.path(), "yuru.zsh", "--zsh");
    let fake = write_fake_yuru(
        dir.path(),
        "fake-yuru",
        r#"printf '%s\n' "$@" > "$YURU_FAKE_ARGS"
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then
    shift
    cat "$1" > "$YURU_FAKE_INPUT"
    break
  fi
  shift
done
printf 'src/main.rs\n'
"#,
    );
    let args_file = dir.path().join("args.txt");
    let input_file = dir.path().join("input.txt");

    let output = StdCommand::new("zsh")
        .args([
            "-fc",
            r#"source "$YURU_SCRIPT"
YURU_BIN="$YURU_FAKE"
FZF_CTRL_T_COMMAND="printf 'src/main.rs\n'"
LBUFFER=""
__yuru_ctrl_t__ 2>/dev/null
print -r -- "$LBUFFER"
cat "$YURU_FAKE_ARGS"
printf '%s\n' "---"
cat "$YURU_FAKE_INPUT""#,
        ])
        .env("YURU_SCRIPT", &script)
        .env("YURU_FAKE", &fake)
        .env("YURU_FAKE_ARGS", &args_file)
        .env("YURU_FAKE_INPUT", &input_file)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("src/main.rs \n"), "stdout={stdout}");
    assert!(stdout.contains("--input\n"), "stdout={stdout}");
    assert!(stdout.ends_with("---\nsrc/main.rs\n"), "stdout={stdout}");
}

#[test]
fn cli_rejects_multiple_shell_integration_flags() {
    command()
        .args(["--bash", "--zsh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "only one of --bash, --zsh, --fish, or --powershell",
        ));
}

fn command() -> Command {
    let mut command = Command::cargo_bin("yuru").unwrap();
    command
        .env_remove("FZF_DEFAULT_OPTS")
        .env_remove("FZF_DEFAULT_OPTS_FILE")
        .env_remove("FZF_DEFAULT_COMMAND")
        .env_remove("YURU_DEFAULT_COMMAND")
        .env_remove("YURU_DEFAULT_OPTS")
        .env_remove("YURU_DEFAULT_OPTS_FILE")
        .env("YURU_CONFIG_FILE", "__yuru_test_no_config__")
        .env_remove("XDG_CONFIG_HOME");
    command
}

#[cfg(unix)]
fn write_shell_script(dir: &std::path::Path, name: &str, flag: &str) -> std::path::PathBuf {
    let output = command().arg(flag).output().unwrap();
    assert!(output.status.success());
    let path = dir.join(name);
    fs::write(&path, output.stdout).unwrap();
    path
}

#[cfg(unix)]
fn write_fake_yuru(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/usr/bin/env bash\n{body}")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}
