use std::io::{self, IsTerminal, Write};

// Format prefix in minimal blue when attached to a TTY
fn blue(text: &str, is_tty: bool) -> String {
    if is_tty {
        format!("\x1b[1;34m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

pub fn warn(msg: &str) {
    let is_tty = io::stderr().is_terminal();
    let _ = writeln!(io::stderr(), "[{}] {}", blue("WARN", is_tty), msg);
}

pub fn info(msg: &str) {
    let is_tty = io::stdout().is_terminal();
    let _ = writeln!(io::stdout(), "[{}] {}", blue("INFO", is_tty), msg);
}
