//! `lp-cli emu serve` exits when the process named by `LP_EMU_PARENT_PID` dies.
//!
//! A walk that is SIGKILLed or timed out runs no exit handler, so the server
//! it started has to notice by itself (`serve/parent_watch.rs`). Without the
//! variable nothing watches, and a terminal `emu serve` behaves as before.
//!
//! The board is a blank `kind=rom-up` chip: no firmware image is needed. The
//! wall timeouts are a safety net, never an input.

#![cfg(unix)]

mod support;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use support::scratch;

/// Longer than the one-second poll plus a loaded box's slack.
const EXIT_NET: Duration = Duration::from_secs(30);

#[test]
fn emu_serve_exits_on_its_own_once_the_named_parent_is_gone() {
    let mut stand_in = Command::new("sleep")
        .arg("600")
        .spawn()
        .expect("a stand-in parent");
    let mut serve = Served::start(Some(stand_in.id()));
    assert!(serve.running(), "the server is up while its parent lives");

    // Kill and reap the stand-in: a zombie still answers `kill(pid, 0)`.
    stand_in.kill().expect("killing the stand-in");
    stand_in.wait().expect("the stand-in exits");

    let exited = serve.wait_exit(EXIT_NET);
    assert!(exited, "the server must exit once its parent is gone");
    let log = serve.log();
    assert!(
        log.contains(&format!(
            "emu serve: parent {} gone — exiting",
            stand_in.id()
        )),
        "the exit says why: {log}"
    );
}

#[test]
fn emu_serve_without_the_variable_keeps_running_after_any_process_exits() {
    let mut stand_in = Command::new("sleep")
        .arg("600")
        .spawn()
        .expect("a stand-in");
    let mut serve = Served::start(None);
    stand_in.kill().expect("killing the stand-in");
    stand_in.wait().expect("the stand-in exits");

    // Far longer than the watcher's poll: it would have gone by now.
    assert!(
        !serve.wait_exit(Duration::from_secs(4)),
        "an unwatched server must keep running: {}",
        serve.log()
    );
    serve.child.kill().expect("killing it ourselves");
    serve.child.wait().expect("reaped");
}

struct Served {
    child: Child,
    log: Arc<Mutex<String>>,
}

impl Served {
    fn start(parent: Option<u32>) -> Served {
        let dir = scratch();
        std::fs::create_dir_all(&dir).expect("the scratch dir");
        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command
            .args(["emu", "serve", "--listen", "127.0.0.1:0"])
            .args(["--board", "c6-a=blank,kind=rom-up"])
            .arg("--state-dir")
            .arg(&dir)
            .env_remove("LP_EMU_PARENT_PID");
        if let Some(pid) = parent {
            command.env("LP_EMU_PARENT_PID", pid.to_string());
        }
        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawning lp-cli emu serve");

        // Read stderr on a thread so the child never blocks on a full pipe;
        // `listening` is the cue the server is up.
        let log = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&log);
        let stderr = child.stderr.take().expect("piped");
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut sink = sink.lock().expect("log lock");
                sink.push_str(&line);
                sink.push('\n');
            }
        });
        let served = Served { child, log };
        let deadline = Instant::now() + EXIT_NET;
        while !served.log().contains("emu serve: listening on") {
            assert!(
                Instant::now() < deadline,
                "never listened: {}",
                served.log()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        served
    }

    fn log(&self) -> String {
        self.log.lock().expect("log lock").clone()
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().expect("try_wait").is_none()
    }

    /// Whether the server exited within `within`.
    fn wait_exit(&mut self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if !self.running() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
