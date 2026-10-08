//! `emu serve` exits when the process that started it dies, if it was asked to.
//!
//! A walk starts `emu serve` detached (`startDoor` in
//! `scripts/emu/emulated-lane.mjs`), so a walk killed by SIGKILL or a harness
//! timeout runs no exit handler and left a server burning a core forever. The
//! walk now names itself in `LP_EMU_PARENT_PID`; once that process is gone the
//! server shuts down as Ctrl-C does. Opt-in: a terminal `emu serve` has no
//! parent to wait for and behaves as before.

use std::time::Duration;

use anyhow::{Result, bail};

/// The environment variable that names the process to outlive.
pub const PARENT_PID_ENV: &str = "LP_EMU_PARENT_PID";

/// How often the named process is looked for.
const POLL: Duration = Duration::from_secs(1);

/// The pid `LP_EMU_PARENT_PID` names, or `None` when it is unset or empty.
pub fn parent_pid_from_env() -> Result<Option<i32>> {
    match std::env::var(PARENT_PID_ENV) {
        Ok(text) if !text.trim().is_empty() => parse_pid(text.trim()).map(Some),
        _ => Ok(None),
    }
}

/// Resolves once `pid` is gone. With no pid it never resolves.
pub async fn gone(pid: Option<i32>) -> i32 {
    let Some(pid) = pid else {
        return std::future::pending().await;
    };
    while alive(pid) {
        tokio::time::sleep(POLL).await;
    }
    pid
}

fn parse_pid(text: &str) -> Result<i32> {
    match text.parse::<i32>() {
        // 0 and negatives would make `kill(2)` address a process group.
        Ok(pid) if pid > 0 => Ok(pid),
        _ => bail!("{PARENT_PID_ENV}=`{text}`: expected a process id"),
    }
}

/// `kill(pid, 0)` sends nothing and says whether the process exists. `EPERM`
/// is a process that exists and is somebody else's.
#[cfg(unix)]
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 performs only the existence and permission checks.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn alive(_pid: i32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pid_is_a_positive_integer() {
        assert_eq!(parse_pid("4242").expect("parses"), 4242);
        assert!(parse_pid("0").is_err());
        assert!(parse_pid("-1").is_err());
        assert!(parse_pid("abc").is_err());
    }
}
