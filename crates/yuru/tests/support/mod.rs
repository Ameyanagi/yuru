#![allow(dead_code)]

use assert_cmd::Command;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub const FIXTURE: &str = include_str!("../fixtures/mixed_paths.txt");

pub fn command() -> Command {
    let mut command = Command::cargo_bin("yuru").unwrap();
    command
        .env_remove("FZF_DEFAULT_OPTS")
        .env_remove("FZF_DEFAULT_OPTS_FILE")
        .env_remove("FZF_DEFAULT_COMMAND")
        .env_remove("YURU_DEFAULT_COMMAND")
        .env_remove("YURU_DEFAULT_OPTS")
        .env_remove("YURU_DEFAULT_OPTS_FILE")
        .env_remove("YURU_FZF_COMPAT")
        .env_remove("YURU_SHELL_BINDINGS")
        .env_remove("YURU_CTRL_T_OPTS")
        .env_remove("YURU_CTRL_R_OPTS")
        .env_remove("YURU_ALT_C_OPTS")
        .env_remove("YURU_COMPLETION_OPTS")
        .env("YURU_CONFIG_FILE", "__yuru_test_no_config__")
        .env_remove("XDG_CONFIG_HOME");
    command
}

#[cfg(unix)]
pub fn write_shell_script(dir: &std::path::Path, name: &str, flag: &str) -> std::path::PathBuf {
    let output = command().arg(flag).output().unwrap();
    assert!(output.status.success());
    let path = dir.join(name);
    fs::write(&path, output.stdout).unwrap();
    path
}

#[cfg(unix)]
pub fn write_fake_yuru(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/usr/bin/env bash\n{body}")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}
