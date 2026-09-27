//! Commands the host sends the board on the control channel, as text lines:
//! readable in a capture and trivial to type from a test page.
//!
//! | command | the board |
//! |---|---|
//! | `hello` | answers `hello <identity>` |
//! | `reset-stats` | zeroes its soak counters, answers `ok reset-stats` |
//! | `stream <count> <min> <max> <seed>` | sends `count` soak messages (0 = until `stop`), sizes from `seed`; answers `stream done sent=<n>` when finished |
//! | `stop` | stops a stream; answers `stream done sent=<n>` |
//! | `stats` | answers `stats k=v …` (its soak counters and its link's) |
//! | `log <n> <len>` | writes `n` log lines of about `len` bytes through its logger |
//! | `stall <ms>` | blocks its executor for `ms` (a shader compile's shape) |
//! | `panic` | panics (the raw-text path) |

use alloc::format;
use alloc::string::String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabCommand {
    Hello,
    ResetStats,
    Stream {
        count: u32,
        min: usize,
        max: usize,
        seed: u64,
    },
    Stop,
    Stats,
    Log {
        n: u32,
        len: usize,
    },
    Stall {
        ms: u32,
    },
    Panic,
}

impl LabCommand {
    pub fn parse(text: &str) -> Option<LabCommand> {
        let mut it = text.split_ascii_whitespace();
        let verb = it.next()?;
        let mut num = || it.next().and_then(|t| t.parse::<u64>().ok());
        Some(match verb {
            "hello" => LabCommand::Hello,
            "reset-stats" => LabCommand::ResetStats,
            "stream" => LabCommand::Stream {
                count: num()? as u32,
                min: num()? as usize,
                max: num()? as usize,
                seed: num()?,
            },
            "stop" => LabCommand::Stop,
            "stats" => LabCommand::Stats,
            "log" => LabCommand::Log {
                n: num()? as u32,
                len: num()? as usize,
            },
            "stall" => LabCommand::Stall { ms: num()? as u32 },
            "panic" => LabCommand::Panic,
            _ => return None,
        })
    }

    pub fn to_text(&self) -> String {
        match self {
            LabCommand::Hello => "hello".into(),
            LabCommand::ResetStats => "reset-stats".into(),
            LabCommand::Stream {
                count,
                min,
                max,
                seed,
            } => format!("stream {count} {min} {max} {seed}"),
            LabCommand::Stop => "stop".into(),
            LabCommand::Stats => "stats".into(),
            LabCommand::Log { n, len } => format!("log {n} {len}"),
            LabCommand::Stall { ms } => format!("stall {ms}"),
            LabCommand::Panic => "panic".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_round_trips_through_its_text() {
        for c in [
            LabCommand::Hello,
            LabCommand::ResetStats,
            LabCommand::Stream {
                count: 0,
                min: 16,
                max: 16384,
                seed: 99,
            },
            LabCommand::Stop,
            LabCommand::Stats,
            LabCommand::Log { n: 200, len: 120 },
            LabCommand::Stall { ms: 3000 },
            LabCommand::Panic,
        ] {
            assert_eq!(LabCommand::parse(&c.to_text()), Some(c));
        }
        assert_eq!(LabCommand::parse("stream 1 2"), None);
        assert_eq!(LabCommand::parse("dance"), None);
    }
}
