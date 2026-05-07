mod support;

use predicates::prelude::*;
use std::fs;
use support::command;

#[test]
fn cli_reads_korean_toml_config_options() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(
        &config,
        "[defaults]\nlang = \"ko\"\n[ko]\nkeyboard = false\n",
    )
    .unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .args(["--filter", "hangeul"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .success()
        .stdout(predicate::eq("한글.txt\n"));

    command()
        .env("YURU_CONFIG_FILE", &config)
        .args(["--filter", "gksrmf"])
        .write_stdin("한글.txt\nnotes.txt\n")
        .assert()
        .failure()
        .stdout(predicate::eq(""));
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
fn cli_reads_toml_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[defaults]\nlang = \"ja\"\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .args(["--filter", "ni"])
        .write_stdin("tests/日本語.txt\nplan.md\n")
        .assert()
        .success()
        .stdout(predicate::eq("tests/日本語.txt\n"));
}
#[test]
fn cli_doctor_reports_missing_config_without_loading_fzf_opts() {
    command()
        .env("FZF_DEFAULT_OPTS", "--definitely-not-a-yuru-option")
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Yuru doctor"))
        .stdout(predicate::str::contains("warn config: missing"))
        .stdout(predicate::str::contains("info default language: plain"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_doctor_reports_toml_default_language() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[defaults]\nlang = \"ja\"\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "ok config: {} (toml)",
            config.display()
        )))
        .stdout(predicate::str::contains("info default language: ja"));
}
#[test]
fn cli_doctor_reports_legacy_default_language() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    fs::write(&config, "--lang zh\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("warn config:"))
        .stdout(predicate::str::contains("legacy shell words"))
        .stdout(predicate::str::contains("info default language: zh"))
        .stderr(predicate::eq(""));
}
#[test]
fn cli_configure_requires_interactive_terminal() {
    command()
        .arg("configure")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "yuru configure requires an interactive terminal",
        ));
}
#[test]
fn cli_toml_config_overrides_safe_fzf_default_opts() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[defaults]\nlimit = 2\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .env("FZF_DEFAULT_OPTS", "--limit 1")
        .args(["--filter", "", "--disabled", "--no-sort"])
        .write_stdin("alpha\nbeta\ngamma\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\nbeta\n"));
}
#[test]
fn cli_yuru_default_opts_override_toml_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[defaults]\nlimit = 2\n").unwrap();

    command()
        .env("YURU_CONFIG_FILE", &config)
        .env("YURU_DEFAULT_OPTS", "--limit 1")
        .args(["--filter", "", "--disabled", "--no-sort"])
        .write_stdin("alpha\nbeta\ngamma\n")
        .assert()
        .success()
        .stdout(predicate::eq("alpha\n"));
}
