//! A path's `d`: SVG path data, read into moves and lines and written back
//! (G1, 2026-10-27: paths as SVG path data).
//!
//! **Read:** `M`/`m` and `L`/`l`, with `H`/`h` and `V`/`v` read as lines;
//! relative commands are made absolute; numbers after a move are lines (SVG's
//! rule). A move after the first is the jumper. `Z`, curves (`C S Q T`) and
//! arcs (`A`) are SVG's too, and are refused by name until they are built,
//! so building them later changes no format. Every point is kept, repeats
//! included: the importer's parser (`import/svg_data.rs`) folds moves into a
//! polyline and drops repeated points, so it can't be this reader.
//!
//! **Written:** absolute, one space between tokens, every command its own
//! letter, each number the shortest text that reads back to the same `f32`
//! (`M 10 10 L 40 10.5`), so a document reads and writes back byte for byte.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::part::PathCommand;

/// Why `d` didn't read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PathDataError {
    /// Not path data, or not a path (it must start with a move).
    Bad(String),
    /// SVG path data this build does not draw yet.
    NotBuilt(char),
}

/// Read `d` into commands.
pub(crate) fn parse_path_data(d: &str) -> Result<Vec<PathCommand>, PathDataError> {
    let mut reader = Reader {
        text: d.as_bytes(),
        at: 0,
    };
    let mut commands = Vec::new();
    let mut current = [0.0f32, 0.0];
    let mut letter: Option<u8> = None;
    while reader.skip_separators() {
        let next = reader.peek();
        if next.is_ascii_alphabetic() {
            reader.at += 1;
            match next {
                b'M' | b'm' | b'L' | b'l' | b'H' | b'h' | b'V' | b'v' => letter = Some(next),
                b'Z' | b'z' | b'C' | b'c' | b'S' | b's' | b'Q' | b'q' | b'T' | b't' | b'A'
                | b'a' => return Err(PathDataError::NotBuilt(next as char)),
                other => {
                    return Err(PathDataError::Bad(format!(
                        "{:?} is not a path command",
                        other as char
                    )));
                }
            }
            if !reader.number_ahead() {
                return Err(PathDataError::Bad(format!(
                    "{:?} has no numbers after it",
                    next as char
                )));
            }
            continue;
        }
        let Some(command) = letter else {
            return Err(PathDataError::Bad(String::from(
                "path data starts with a number, not a move",
            )));
        };
        if commands.is_empty() && !matches!(command, b'M' | b'm') {
            return Err(PathDataError::Bad(String::from(
                "path data must start with a move (M)",
            )));
        }
        let relative = command.is_ascii_lowercase();
        // Absolute numbers are taken as written (no `0 + x`, which would
        // lose a `-0`).
        let along = |from: f32, n: f32| if relative { from + n } else { n };
        let point = match command.to_ascii_uppercase() {
            b'M' | b'L' => {
                let x = reader.number()?;
                let y = reader.number()?;
                [along(current[0], x), along(current[1], y)]
            }
            b'H' => [along(current[0], reader.number()?), current[1]],
            _ => [current[0], along(current[1], reader.number()?)],
        };
        if command.eq_ignore_ascii_case(&b'M') {
            commands.push(PathCommand::Move(point));
            // Numbers after a move are lines (SVG): relative after `m`.
            letter = Some(if relative { b'l' } else { b'L' });
        } else {
            commands.push(PathCommand::Line(point));
        }
        current = point;
    }
    if commands.is_empty() {
        return Err(PathDataError::Bad(String::from("path data is empty")));
    }
    Ok(commands)
}

/// Write commands as `d`. `Err` names a number that isn't finite: no text
/// reads back to it.
pub(crate) fn write_path_data(commands: &[PathCommand]) -> Result<String, String> {
    let mut d = String::new();
    for command in commands {
        let (letter, [x, y]) = match *command {
            PathCommand::Move(p) => ('M', p),
            PathCommand::Line(p) => ('L', p),
        };
        if !x.is_finite() || !y.is_finite() {
            return Err(format!("a path point [{x}, {y}] is not a finite number"));
        }
        if !d.is_empty() {
            d.push(' ');
        }
        // `{}` on an f32 is the shortest text that reads back to it.
        d.push_str(&format!("{letter} {x} {y}"));
    }
    Ok(d)
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    /// Skip whitespace and commas; is anything left?
    fn skip_separators(&mut self) -> bool {
        while self.at < self.text.len()
            && (self.text[self.at].is_ascii_whitespace() || self.text[self.at] == b',')
        {
            self.at += 1;
        }
        self.at < self.text.len()
    }

    fn peek(&self) -> u8 {
        self.text[self.at]
    }

    fn number_ahead(&mut self) -> bool {
        self.skip_separators() && matches!(self.peek(), b'-' | b'+' | b'.' | b'0'..=b'9')
    }

    /// One number: sign, digits, a fraction, an exponent.
    fn number(&mut self) -> Result<f32, PathDataError> {
        if !self.number_ahead() {
            return Err(PathDataError::Bad(String::from(
                "a command is missing a number",
            )));
        }
        let start = self.at;
        if matches!(self.peek(), b'-' | b'+') {
            self.at += 1;
        }
        let mut digits = self.digits();
        if self.at < self.text.len() && self.text[self.at] == b'.' {
            self.at += 1;
            digits += self.digits();
        }
        if digits > 0 && self.at < self.text.len() && matches!(self.text[self.at], b'e' | b'E') {
            let mark = self.at;
            self.at += 1;
            if self.at < self.text.len() && matches!(self.text[self.at], b'-' | b'+') {
                self.at += 1;
            }
            if self.digits() == 0 {
                self.at = mark;
            }
        }
        let text = core::str::from_utf8(&self.text[start..self.at]).unwrap_or("");
        if digits == 0 {
            return Err(PathDataError::Bad(format!("{text:?} is not a number")));
        }
        text.parse::<f32>()
            .map_err(|_| PathDataError::Bad(format!("{text:?} is not a number")))
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.at < self.text.len() && self.text[self.at].is_ascii_digit() {
            self.at += 1;
        }
        self.at - start
    }
}
