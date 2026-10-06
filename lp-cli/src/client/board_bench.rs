//! The desk's board bench: `board`, from
//! <https://github.com/PhotomancerArt/lp-board-bench>.
//!
//! Several agent sessions share the USB boards on Yona's desk. `board` keeps
//! the registry (each board's mark, slug, role and chip) and short expiring
//! leases, so a flash or a reset never lands on a board someone else is
//! using. This module is lp-cli's only contact with it: it runs the `board`
//! command and reads its exit codes and JSON.
//!
//! `board` is a desk tool, not a dependency. When it is not installed (CI,
//! another machine) every call here is a silent no-op, and a `board` that
//! fails in a way it does not explain (a broken registry) is a warning, never
//! a refusal: the courtesy lock must not stop a desk that has no bench.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};
use serde::Deserialize;

/// `board check` exit codes (lp-board-bench `src/check.ts`).
const EXIT_OK: i32 = 0;
const EXIT_HELD: i32 = 3;
const EXIT_ART: i32 = 4;

/// One board as `board list --json` reports it. Only the fields lp-cli reads;
/// the bench only ever adds fields.
#[derive(Debug, Clone, Deserialize)]
pub struct BenchBoard {
    pub slug: Option<String>,
    pub mark: Option<String>,
    pub mac: Option<String>,
    pub role: Option<String>,
    pub chip: Option<String>,
    pub port: Option<String>,
    pub lease: Option<BenchLease>,
    /// A LightPlayer board id: what `hardware desk-images` draws.
    pub lp_board: Option<String>,
    /// A LightPlayer project: the piece `hardware desk-images` draws.
    pub lp_project: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BenchLease {
    pub holder: String,
    pub purpose: String,
    pub expires: String,
}

#[derive(Debug, Deserialize)]
struct BenchState {
    boards: Vec<BenchBoard>,
}

/// Refuse when `board` says the board on `port` is someone else's (or an art
/// piece nobody took on purpose). `chip` is what a probe just read, so the
/// bench can shout when its registry disagrees. `holder` overrides
/// `$BOARD_HOLDER` as who is asking.
pub fn check(port: &str, chip: Option<&str>, holder: Option<&str>) -> Result<()> {
    let Some(board) = board_binary() else {
        return Ok(());
    };
    let mut args: Vec<OsString> = vec!["check".into(), port.into()];
    if let Some(chip) = chip {
        args.extend(["--chip".into(), chip.into()]);
    }
    if let Some(holder) = holder {
        args.extend(["--as".into(), holder.into()]);
    }
    let output = match Command::new(&board).args(&args).output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!("warning: could not run {}: {err}", board.display());
            return Ok(());
        }
    };
    let said = String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_owned();
    match output.status.code() {
        Some(EXIT_OK) => {
            if !said.is_empty() {
                eprintln!("board: {said}");
            }
            Ok(())
        }
        Some(EXIT_HELD) | Some(EXIT_ART) => bail!(
            "refusing {port}: {said}\n(the desk's board leases: `board list`; \
             say who you are with BOARD_HOLDER=<who>)"
        ),
        _ => {
            eprintln!("warning: `board check {port}` could not decide, going ahead: {said}");
            Ok(())
        }
    }
}

/// Lease the board on `port` for `for_text` (`"<who>: <why>"`). Unlike
/// [`check`], this needs `board`: asking for a lease and silently not getting
/// one would be worse than failing.
pub fn take(port: &str, for_text: &str, minutes: Option<u32>) -> Result<()> {
    let Some(board) = board_binary() else {
        bail!(
            "--lease needs `board` (github.com/PhotomancerArt/lp-board-bench) on PATH, or BOARD_BIN"
        );
    };
    let mut command = Command::new(&board);
    command.args(["take", port, "--for", for_text]);
    if let Some(minutes) = minutes {
        command.args(["--minutes", &minutes.to_string()]);
    }
    let output = command.output()?;
    let said = String::from_utf8_lossy(&output.stderr)
        .trim_end()
        .to_owned();
    if output.status.success() {
        eprintln!("board: {said}");
        Ok(())
    } else {
        bail!("could not lease {port}: {said}")
    }
}

/// The bench's view of every board, or `None` without a working `board`.
pub fn list() -> Option<Vec<BenchBoard>> {
    let board = board_binary()?;
    let output = Command::new(&board)
        .args(["list", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "warning: `board list` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
        return None;
    }
    match serde_json::from_slice::<BenchState>(&output.stdout) {
        Ok(state) => Some(state.boards),
        Err(err) => {
            eprintln!("warning: could not read `board list --json`: {err}");
            None
        }
    }
}

/// The holder part of a `--for "<who>: <why>"` string — the bench's own rule.
pub fn holder_of(for_text: &str) -> &str {
    for_text.split(':').next().unwrap_or(for_text).trim()
}

/// `a0f2…`, `A0-F2-…`, `a0:f2:…` → `A0:F2:…`.
pub fn normalize_mac(text: &str) -> Option<String> {
    let hex: String = text
        .chars()
        .filter(|ch| !matches!(ch, ':' | '-' | '.'))
        .collect::<String>()
        .to_ascii_uppercase();
    if hex.len() != 12 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// `$BOARD_BIN` when set (an empty value turns the bench off), else the
/// first `board` on `PATH`.
fn board_binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("BOARD_BIN") {
        let path = PathBuf::from(explicit);
        return is_executable(&path).then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("board"))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    /// The env is process-wide; these tests take turns with it.
    static ENV: Mutex<()> = Mutex::new(());

    #[test]
    fn a_free_or_own_board_passes() {
        let _guard = ENV.lock().unwrap();
        let stub = Stub::new(0, "FC6 fixture-c6 is free");
        assert!(check("/dev/cu.usbmodem1", None, None).is_ok());
        drop(stub);
    }

    #[test]
    fn a_held_board_is_refused_with_the_bench_message() {
        let _guard = ENV.lock().unwrap();
        let _stub = Stub::new(
            3,
            "FC6 fixture-c6 is held by ota until 20:00 (12 min left): soak",
        );
        let err = check("/dev/cu.usbmodem1", None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("held by ota until 20:00"), "{err}");
    }

    #[test]
    fn an_art_board_not_taken_on_purpose_is_refused() {
        let _guard = ENV.lock().unwrap();
        let _stub = Stub::new(
            4,
            "CHK art-choker is an art piece; take it on purpose first",
        );
        assert!(check("/dev/cu.usbmodem1", None, None).is_err());
    }

    #[test]
    fn a_bench_that_cannot_decide_only_warns() {
        let _guard = ENV.lock().unwrap();
        let _stub = Stub::new(5, "no board matches");
        assert!(check("/dev/cu.usbmodem1", None, None).is_ok());
        let _broken = Stub::new(1, "boards.toml: not valid TOML");
        assert!(check("/dev/cu.usbmodem1", None, None).is_ok());
    }

    #[test]
    fn no_bench_is_a_silent_no_op_for_checks_but_an_error_for_a_lease() {
        let _guard = ENV.lock().unwrap();
        let _env = EnvVar::set("BOARD_BIN", "/nonexistent/board");
        assert!(check("/dev/cu.usbmodem1", None, None).is_ok());
        assert!(list().is_none());
        assert!(take("/dev/cu.usbmodem1", "me: test", None).is_err());
    }

    #[test]
    fn check_passes_the_probed_chip_and_the_holder() {
        let _guard = ENV.lock().unwrap();
        let stub = Stub::new(0, "ok");
        check("/dev/cu.usbmodem1", Some("esp32s3"), Some("ota")).unwrap();
        assert_eq!(
            stub.args(),
            "check /dev/cu.usbmodem1 --chip esp32s3 --as ota"
        );
    }

    #[test]
    fn list_reads_the_bench_json() {
        let _guard = ENV.lock().unwrap();
        let _stub = Stub::with_stdout(
            0,
            r#"{"hubsAvailable":true,"boards":[{"slug":"fixture-c6","mark":"FC6","mac":"02:00:00:00:00:01",
               "role":"fixture","chip":"esp32c6","port":"/dev/cu.usbmodem1","extra":1,
               "lease":{"holder":"ota","purpose":"soak","expires":"2026-10-05T20:00:00Z","acquired":"x"}}]}"#,
        );
        let boards = list().unwrap();
        assert_eq!(boards[0].mark.as_deref(), Some("FC6"));
        assert_eq!(boards[0].slug.as_deref(), Some("fixture-c6"));
        assert_eq!(boards[0].lease.as_ref().unwrap().holder, "ota");
    }

    #[test]
    fn macs_normalise_from_any_separator() {
        assert_eq!(
            normalize_mac("a0f26287b48c").as_deref(),
            Some("A0:F2:62:87:B4:8C")
        );
        assert_eq!(
            normalize_mac("02-00-00-00-00-01").as_deref(),
            Some("02:00:00:00:00:01")
        );
        assert_eq!(normalize_mac("SN234567892"), None);
        assert_eq!(normalize_mac("02:00:00:00:00"), None);
    }

    #[test]
    fn the_holder_is_the_text_before_the_first_colon() {
        assert_eq!(
            holder_of("ota-director: power-cut: round 2"),
            "ota-director"
        );
        assert_eq!(holder_of(" yona "), "yona");
    }

    /// A fake `board`: a shell script that records its arguments and exits
    /// with `code`, saying `stderr`. Installed through `BOARD_BIN`.
    struct Stub {
        dir: tempfile::TempDir,
        _env: EnvVar,
    }

    impl Stub {
        fn new(code: i32, stderr: &str) -> Self {
            Self::script(code, stderr, "")
        }

        fn with_stdout(code: i32, stdout: &str) -> Self {
            Self::script(code, "", stdout)
        }

        fn script(code: i32, stderr: &str, stdout: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("board");
            let args = dir.path().join("args");
            std::fs::write(dir.path().join("stdout"), stdout).unwrap();
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho \"$*\" > '{}'\ncat '{}'\necho '{}' >&2\nexit {code}\n",
                    args.display(),
                    dir.path().join("stdout").display(),
                    stderr.replace('\'', "")
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let env = EnvVar::set("BOARD_BIN", path.to_str().unwrap());
            Self { dir, _env: env }
        }

        fn args(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("args"))
                .unwrap()
                .trim()
                .to_owned()
        }
    }

    struct EnvVar {
        key: &'static str,
        old: Option<OsString>,
    }

    impl EnvVar {
        fn set(key: &'static str, value: &str) -> Self {
            let old = std::env::var_os(key);
            // SAFETY: tests touching the environment hold the ENV mutex.
            unsafe { std::env::set_var(key, value) };
            Self { key, old }
        }
    }

    impl Drop for EnvVar {
        fn drop(&mut self) {
            // SAFETY: as above.
            unsafe {
                match &self.old {
                    Some(old) => std::env::set_var(self.key, old),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }
}
