//! `emu serve` exits once the process named by `LP_EMU_PARENT_PID` is gone,
//! and without the variable nothing watches (`serve/parent_watch.rs`).
//! The board is a blank `kind=rom-up` chip, so no firmware image is needed.

#![cfg(unix)]

mod support;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn emu_serve_exits_on_its_own_once_the_named_parent_is_gone() {
    let mut stand_in = sleeper();
    let mut serve = Served::start(Some(stand_in.id()));
    assert!(!serve.exits_within(2), "up while its parent lives");

    // Kill AND reap the stand-in: a zombie still answers `kill(pid, 0)`.
    stand_in.kill().expect("killing the stand-in");
    stand_in.wait().expect("reaped");

    assert!(serve.exits_within(30), "must exit once its parent is gone");
    let said = format!("emu serve: parent {} gone — exiting", stand_in.id());
    assert!(serve.log().contains(&said), "says why: {}", serve.log());
}

#[test]
fn emu_serve_without_the_variable_keeps_running_after_any_process_exits() {
    let mut stand_in = sleeper();
    let mut serve = Served::start(None);
    stand_in.kill().expect("killing the stand-in");
    stand_in.wait().expect("reaped");

    // Far longer than the watcher's one-second poll.
    assert!(
        !serve.exits_within(4),
        "unwatched, it runs: {}",
        serve.log()
    );
}

fn sleeper() -> Child {
    Command::new("sleep")
        .arg("600")
        .spawn()
        .expect("a stand-in parent")
}

/// A running `emu serve`, killed on drop.
struct Served {
    child: Child,
    log: Arc<Mutex<String>>,
}

impl Served {
    fn start(parent: Option<u32>) -> Served {
        let dir = support::scratch();
        std::fs::create_dir_all(&dir).expect("the scratch dir");
        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command
            .args(["emu", "serve", "--listen", "127.0.0.1:0"])
            .args(["--board", "c6-a=blank,kind=rom-up", "--state-dir"])
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

        // Drain stderr on a thread, so the child never blocks on a full pipe.
        let log = Arc::new(Mutex::new(String::new()));
        let (sink, stderr) = (Arc::clone(&log), child.stderr.take().expect("piped"));
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                sink.lock().expect("lock").push_str(&(line + "\n"));
            }
        });
        let served = Served { child, log };
        let deadline = Instant::now() + Duration::from_secs(30);
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
        self.log.lock().expect("lock").clone()
    }

    /// Whether the server exited within `secs`.
    fn exits_within(&mut self, secs: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            if self.child.try_wait().expect("try_wait").is_some() {
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
