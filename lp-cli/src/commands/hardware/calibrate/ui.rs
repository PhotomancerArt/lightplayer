use std::io::{IsTerminal, stdout};

pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const RESET: &str = "\x1b[0m";

pub fn paint(code: &str, text: &str) -> String {
    if stdout().is_terminal() {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn section(title: &str) {
    println!();
    println!("{}", paint(BOLD, title));
}
