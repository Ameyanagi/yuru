use std::io::{self, Write};

use anyhow::{Context, Result};

pub(crate) fn print_split_shell_words(words: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for word in parse_shell_words(words)? {
        stdout.write_all(word.as_bytes())?;
        stdout.write_all(&[0])?;
    }
    Ok(())
}

pub(crate) fn split_shell_words(input: &str) -> impl Iterator<Item = String> + '_ {
    shlex::split(input).unwrap_or_default().into_iter()
}

pub(crate) fn parse_shell_words(input: &str) -> Result<Vec<String>> {
    shlex::split(input).with_context(|| "failed to parse shell words")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_hyphen_values() {
        assert_eq!(
            parse_shell_words("--preview 'file {}' --bind ctrl-j:preview-down").unwrap(),
            vec!["--preview", "file {}", "--bind", "ctrl-j:preview-down"]
        );
    }
}
