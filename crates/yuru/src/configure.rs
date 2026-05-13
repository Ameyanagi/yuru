use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::{
    config::{config_bool, config_string, config_string_list, yuru_config_source, ConfigSource},
    doctor::doctor_default_lang,
    options::{default_preview_text_extensions, parse_preview_text_extensions},
    shell::shell_config_from_value,
};

pub(crate) fn configure_interactive() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("yuru configure requires an interactive terminal");
    }

    let path = config_path_for_write();
    let mut value = read_config_for_write(&path)?;
    let current_lang = config_string(&value, &["defaults", "lang"])
        .or_else(|| doctor_default_lang(yuru_config_source().as_ref()))
        .unwrap_or_else(|| "ja".to_string());
    let current_load_fzf = config_string(&value, &["defaults", "load_fzf_defaults"])
        .or_else(|| {
            config_bool(&value, &["fzf", "safe_default_opts"]).map(|safe| {
                if safe {
                    "safe".to_string()
                } else {
                    "all".to_string()
                }
            })
        })
        .unwrap_or_else(|| "safe".to_string());
    let current_fzf_compat = config_string(&value, &["defaults", "fzf_compat"])
        .or_else(|| config_string(&value, &["fzf", "unsupported_options"]))
        .unwrap_or_else(|| "warn".to_string());
    let current_preview_command =
        config_string(&value, &["preview", "command"]).unwrap_or_else(|| "auto".to_string());
    let current_preview_text_extensions =
        config_string_list(&value, &["preview", "text_extensions"])
            .unwrap_or_else(default_preview_text_extensions)
            .join(",");
    let current_preview_image_protocol =
        config_string(&value, &["preview", "image_protocol"]).unwrap_or_else(|| "none".to_string());
    let current_shell = shell_config_from_value(&value);

    println!("Yuru configure");
    println!("Config: {}", path.display());
    println!("Press Enter to keep the shown default.");

    let lang = prompt_choice(
        "Default language",
        &current_lang,
        &["plain", "ja", "ko", "zh", "all", "auto", "none"],
    )?;
    let load_fzf_defaults = prompt_choice(
        "Load FZF_DEFAULT_OPTS",
        &current_load_fzf,
        &["never", "safe", "all"],
    )?;
    let fzf_compat = prompt_choice(
        "Unsupported fzf options",
        &current_fzf_compat,
        &["strict", "warn", "ignore"],
    )?;
    let preview_image_protocol = prompt_choice(
        "Preview image protocol",
        &current_preview_image_protocol,
        &["none", "auto", "halfblocks", "sixel", "kitty", "iterm2"],
    )?;
    let preview_command = prompt_preview_command(&current_preview_command)?;
    let preview_text_extensions = prompt_string(
        "Preview text extensions",
        &current_preview_text_extensions,
        "Comma-separated extensions used by the built-in preview command.",
    )?
    .unwrap_or_else(|| current_preview_text_extensions.clone());
    let path_backend = prompt_choice(
        "Shell path backend",
        &current_shell.path_backend,
        &["auto", "fd", "fdfind", "find"],
    )?;
    let bindings = prompt_bindings_value(&current_shell.bindings)?;
    let ctrl_t_command = prompt_string(
        "CTRL-T command",
        &current_shell.ctrl_t_command,
        "Use 'none' to disable this binding's candidate command.",
    )?;
    let ctrl_t_opts = prompt_string(
        "CTRL-T options",
        &current_shell.ctrl_t_opts,
        "Use 'none' to disable extra options such as preview.",
    )?;
    let alt_c_command = prompt_string(
        "ALT-C command",
        &current_shell.alt_c_command,
        "Use 'none' to disable this binding's candidate command.",
    )?;
    let alt_c_opts = prompt_string(
        "ALT-C options",
        &current_shell.alt_c_opts,
        "Use 'none' to disable extra options such as preview.",
    )?;

    {
        let defaults = ensure_toml_table(&mut value, "defaults");
        if lang == "none" {
            defaults.remove("lang");
        } else {
            defaults.insert("lang".to_string(), toml::Value::String(lang));
        }
        defaults.insert(
            "load_fzf_defaults".to_string(),
            toml::Value::String(load_fzf_defaults),
        );
        defaults.insert("fzf_compat".to_string(), toml::Value::String(fzf_compat));
    }

    {
        let preview = ensure_toml_table(&mut value, "preview");
        preview.insert("command".to_string(), toml::Value::String(preview_command));
        preview.insert(
            "text_extensions".to_string(),
            toml::Value::Array(
                parse_preview_text_extensions(&preview_text_extensions)
                    .into_iter()
                    .map(toml::Value::String)
                    .collect(),
            ),
        );
        preview.insert(
            "image_protocol".to_string(),
            toml::Value::String(preview_image_protocol),
        );
    }

    {
        let shell = ensure_toml_table(&mut value, "shell");
        shell.insert("bindings".to_string(), toml::Value::String(bindings));
        shell.insert(
            "path_backend".to_string(),
            toml::Value::String(path_backend),
        );
        set_optional_toml_string(shell, "ctrl_t_command", ctrl_t_command);
        set_optional_toml_string(shell, "ctrl_t_opts", ctrl_t_opts);
        set_optional_toml_string(shell, "alt_c_command", alt_c_command);
        set_optional_toml_string(shell, "alt_c_opts", alt_c_opts);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(&value).context("failed to serialize config")?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    println!("Updated {}", path.display());
    println!("Restart your shell or reload your profile for shell binding changes.");
    Ok(())
}

fn config_path_for_write() -> PathBuf {
    if let Ok(path) = std::env::var("YURU_CONFIG_FILE") {
        return PathBuf::from(path);
    }
    if let Some(ConfigSource::Toml(path)) = yuru_config_source() {
        return path;
    }
    default_config_path()
}

fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("yuru").join("config.toml");
        }
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(config_home).join("yuru").join("config.toml")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join(".config")
            .join("yuru")
            .join("config.toml")
    }
}

fn read_config_for_write(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn ensure_toml_table<'a>(
    value: &'a mut toml::Value,
    key: &str,
) -> &'a mut toml::map::Map<String, toml::Value> {
    if !value.is_table() {
        *value = toml::Value::Table(toml::map::Map::new());
    }
    let root = value.as_table_mut().expect("root config is a TOML table");
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::map::Map::new());
    }
    entry
        .as_table_mut()
        .expect("config section is a TOML table")
}

fn set_optional_toml_string(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        table.insert(key.to_string(), toml::Value::String(value));
    } else {
        table.remove(key);
    }
}

fn prompt_choice(prompt: &str, current: &str, choices: &[&str]) -> Result<String> {
    loop {
        print!("{prompt} [{}] ({current}): ", choices.join("/"));
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        let answer = answer.trim();
        let value = if answer.is_empty() { current } else { answer };
        if choices.contains(&value) {
            return Ok(value.to_string());
        }
        println!("Please enter one of: {}", choices.join(", "));
    }
}

fn prompt_bindings_value(current: &str) -> Result<String> {
    loop {
        print!("Shell bindings [all/custom/none/list] ({current}): ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        let answer = answer.trim();
        let value = if answer.is_empty() { current } else { answer };
        match value {
            "all" | "none" => return Ok(value.to_string()),
            "custom" => return prompt_custom_bindings(),
            _ if validate_binding_value(value) => return Ok(value.to_string()),
            _ => {
                println!(
                    "Please enter all, custom, none, or a comma-separated list of ctrl-t, ctrl-r, alt-c, completion."
                );
            }
        }
    }
}

fn validate_binding_value(value: &str) -> bool {
    value.split(',').all(|item| {
        matches!(
            item.trim(),
            "ctrl-t" | "ctrl-r" | "alt-c" | "completion" | "tab" | "path-completion"
        )
    })
}

fn prompt_custom_bindings() -> Result<String> {
    let mut selected = Vec::new();
    if prompt_yes_no("Enable CTRL-T file search?", true)? {
        selected.push("ctrl-t");
    }
    if prompt_yes_no("Enable CTRL-R history search?", true)? {
        selected.push("ctrl-r");
    }
    if prompt_yes_no("Enable ALT-C directory jump?", true)? {
        selected.push("alt-c");
    }
    if prompt_yes_no("Enable **<TAB> path completion?", true)? {
        selected.push("completion");
    }
    Ok(if selected.is_empty() {
        "none".to_string()
    } else {
        selected.join(",")
    })
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "Y/n" } else { "y/N" };
    loop {
        print!("{prompt} [{suffix}]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        match answer.trim() {
            "" => return Ok(default_yes),
            "y" | "Y" | "yes" | "YES" | "Yes" => return Ok(true),
            "n" | "N" | "no" | "NO" | "No" => return Ok(false),
            _ => println!("Please enter yes or no."),
        }
    }
}

fn prompt_string(prompt: &str, current: &str, help: &str) -> Result<Option<String>> {
    println!("{help}");
    print!("{prompt} ({current}): ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() {
        Ok(Some(current.to_string()))
    } else if answer == "none" {
        Ok(Some(String::new()))
    } else {
        Ok(Some(answer.to_string()))
    }
}

fn prompt_preview_command(current: &str) -> Result<String> {
    println!(
        "Use 'auto' for built-in bat/cat text preview, 'none' to disable, or a shell command."
    );
    print!("Preview command ({current}): ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() {
        Ok(current.to_string())
    } else {
        Ok(answer.to_string())
    }
}
