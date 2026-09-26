//! `lp-cli record serve`, end to end: the shipped binary as a child
//! process, driven with raw HTTP the way a browser would — a CORS /
//! private-network preflight, then POSTs from two page sessions.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[test]
fn record_serve_files_each_session_and_answers_the_preflight() {
    let out = tempfile::tempdir().expect("temp dir");
    let serve = Serve::start(out.path());

    // The preflight a public https page sends before reaching loopback.
    let preflight = serve.request(
        "OPTIONS /ingest HTTP/1.1\r\nHost: x\r\nOrigin: https://lightplayer.app\r\n\
         Access-Control-Request-Method: POST\r\n\
         Access-Control-Request-Private-Network: true\r\n\r\n",
    );
    assert!(preflight.starts_with("HTTP/1.1 204"), "{preflight}");
    let lower = preflight.to_ascii_lowercase();
    assert!(
        lower.contains("access-control-allow-origin: *"),
        "{preflight}"
    );
    assert!(
        lower.contains("access-control-allow-methods: post, options"),
        "{preflight}"
    );
    assert!(
        lower.contains("access-control-allow-headers: content-type"),
        "{preflight}"
    );
    assert!(
        lower.contains("access-control-allow-private-network: true"),
        "{preflight}"
    );

    // Two sessions, interleaved; the second batch of A lacks its newline.
    let a1 = "{\"seq\":0,\"kind\":\"session\"}\n{\"seq\":1,\"kind\":\"route\"}\n";
    let b1 = "{\"seq\":0,\"kind\":\"session\"}\n";
    let a2 = "{\"seq\":2,\"kind\":\"toast\"}";
    for (session, body) in [("aaaa1111", a1), ("bbbb2222", b1), ("aaaa1111", a2)] {
        let reply = serve.post(&format!("/ingest?session={session}"), body);
        assert!(reply.starts_with("HTTP/1.1 204"), "{reply}");
        assert!(
            reply
                .to_ascii_lowercase()
                .contains("access-control-allow-origin: *"),
            "{reply}"
        );
    }

    let unknown = serve.post("/elsewhere", "x");
    assert!(unknown.starts_with("HTTP/1.1 404"), "{unknown}");

    let mut files: Vec<_> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 2, "one file per session: {files:?}");
    let named = |session: &str| {
        files
            .iter()
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(&format!("-{session}.jsonl"))
            })
            .unwrap_or_else(|| panic!("no file for {session}: {files:?}"))
            .clone()
    };
    let a = std::fs::read_to_string(named("aaaa1111")).unwrap();
    assert_eq!(a, format!("{a1}{a2}\n"));
    let b = std::fs::read_to_string(named("bbbb2222")).unwrap();
    assert_eq!(b, b1);

    // The stamp prefix is `YYYYMMDD-HHMMSS-`.
    let name = named("bbbb2222")
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let stamp = &name[..15];
    assert!(
        stamp
            .chars()
            .enumerate()
            .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() }),
        "{name}"
    );
}

struct Serve {
    child: Child,
    port: u16,
}

impl Serve {
    fn start(out: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["record", "serve", "--port", "0", "--out"])
            .arg(out)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn lp-cli record serve");
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let port = loop {
            let line = lines
                .next()
                .expect("record serve exited before printing its sink")
                .unwrap();
            if let Some(rest) = line.trim().strip_prefix("sink") {
                let url = rest.trim();
                let port = url
                    .trim_start_matches("http://127.0.0.1:")
                    .trim_end_matches("/ingest");
                break port.parse().expect("a port in the sink URL");
            }
        };
        // Keep draining stdout (the per-session lines) so the child never
        // blocks on a full pipe.
        std::thread::spawn(move || for _ in lines {});
        Self { child, port }
    }

    fn request(&self, raw: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    }

    fn post(&self, path: &str, body: &str) -> String {
        self.request(&format!(
            "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: text/plain;charset=UTF-8\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    }
}

/// The child dies with the test, pass or panic.
impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
