mod support;

use predicates::prelude::*;
#[cfg(unix)]
use std::process::Command as StdCommand;
use support::command;
#[cfg(unix)]
use support::{write_fake_yuru, write_shell_script};

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
        .stdout(predicate::str::contains("command -v fd"))
        .stdout(predicate::str::contains("command -v fdfind"))
        .stdout(predicate::str::contains("command find"))
        .stdout(predicate::str::contains("--fzf-compat ignore"))
        .stdout(predicate::str::contains("__yuru_setup_completion__"))
        .stdout(predicate::str::contains("complete -D"))
        .stdout(predicate::str::contains("**<TAB>"))
        .stdout(predicate::str::contains("file,dir,follow,hidden").not());
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
fn zsh_ctrl_t_streams_command_candidates() {
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
cat > "$YURU_FAKE_INPUT"
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
YURU_CTRL_T_COMMAND="printf 'src/main.rs\n'"
YURU_CTRL_T_OPTS="--preview 'fzf-preview.sh {}'"
LBUFFER=""
__yuru_ctrl_t__
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
    assert!(stdout.contains("--fzf-compat\nignore\n"), "stdout={stdout}");
    assert!(
        stdout.contains("--preview\nfzf-preview.sh {}\n"),
        "stdout={stdout}"
    );
    assert!(!stdout.contains("--input\n"), "stdout={stdout}");
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
