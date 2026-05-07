mod support;

use predicates::prelude::*;
use std::fs;
use std::path::MAIN_SEPARATOR;
use support::command;

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
#[cfg(unix)]
#[test]
fn cli_walker_skips_symlink_loops_when_following_links() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("loop").join("nested")).unwrap();
    std::os::unix::fs::symlink("..", dir.path().join("loop").join("nested").join("back")).unwrap();
    fs::write(dir.path().join("alpha.txt"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args(["--filter", "alpha", "--walker", "file,follow,hidden"])
        .assert()
        .success()
        .stdout(predicate::eq("alpha.txt\n"));
}
#[test]
fn cli_walker_respects_gitignore() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(dir.path().join("ignored.txt"), "").unwrap();

    command()
        .current_dir(dir.path())
        .args(["--filter", "ignored", "--walker", "file"])
        .assert()
        .failure()
        .stdout(predicate::eq(""));
}
