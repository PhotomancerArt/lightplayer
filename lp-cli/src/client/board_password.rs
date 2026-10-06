//! A locked board's password, for a `lan:` link: from one line of stdin
//! (`--password-stdin`) or `LP_PASSWORD`, never from argv (shell history and
//! `ps` would keep it). Nothing here prints it.
//!
//! Only a `lan:` link reads one. USB is trusted, and a password a command is
//! given for any other host is never read (stdin stays untouched).

use std::io::BufRead;

use anyhow::{Context, Result};
use clap::Args;
use lpa_client::HostSpecifier;
use lpa_client::transport_lan::BoardPassword;

/// The environment variable a board's password may come from.
pub const BOARD_PASSWORD_ENV: &str = "LP_PASSWORD";

/// `--password-stdin`, for commands that take a board address.
#[derive(Debug, Clone, Copy, Default, Args)]
pub struct BoardPasswordArgs {
    /// For a locked board at a lan: address, read its password from one line
    /// of stdin (never from argv). LP_PASSWORD works too.
    #[arg(long = "password-stdin")]
    pub password_stdin: bool,
}

impl BoardPasswordArgs {
    /// The password for `spec`: `None` for anything but a `lan:` board, or
    /// when none was given.
    pub fn resolve(self, spec: &HostSpecifier) -> Result<Option<BoardPassword>> {
        if !spec.is_lan() {
            return Ok(None);
        }
        if self.password_stdin {
            return read_password_line(&mut std::io::stdin().lock()).map(Some);
        }
        Ok(board_password_from_env())
    }
}

/// `LP_PASSWORD`, when it is set and not empty.
pub fn board_password_from_env() -> Option<BoardPassword> {
    std::env::var(BOARD_PASSWORD_ENV)
        .ok()
        .filter(|password| !password.is_empty())
        .map(BoardPassword::new)
}

/// One line of `input`, without its line ending.
fn read_password_line(input: &mut impl BufRead) -> Result<BoardPassword> {
    let mut line = String::new();
    input
        .read_line(&mut line)
        .context("reading the board's password from stdin")?;
    let password = line.trim_end_matches(['\n', '\r']);
    if password.is_empty() {
        anyhow::bail!("--password-stdin: stdin gave no password");
    }
    Ok(BoardPassword::new(password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_line_loses_its_line_ending_and_an_empty_one_is_refused() {
        let password = read_password_line(&mut &b"camp fire\r\nnext"[..]).unwrap();
        assert_eq!(password.as_bytes(), b"camp fire");
        assert!(read_password_line(&mut &b"\n"[..]).is_err());
    }

    #[test]
    fn only_a_lan_board_reads_a_password() {
        let args = BoardPasswordArgs {
            password_stdin: true,
        };
        // Would block on stdin if it read it.
        assert_eq!(
            args.resolve(&HostSpecifier::parse("serial:auto").unwrap())
                .unwrap(),
            None
        );
    }
}
