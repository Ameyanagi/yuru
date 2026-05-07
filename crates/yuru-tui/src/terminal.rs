use std::io;

use crossterm::{
    cursor::Show,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};

pub(crate) struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), Show, LeaveAlternateScreen);
    }
}
