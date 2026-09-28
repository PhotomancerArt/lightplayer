//! `lp-cli record serve`: a tiny HTTP receiver for Studio's session
//! recorder.
//!
//! The page POSTs batches of JSONL lines to `/ingest?session=<id>` every
//! 250 ms (and one `sendBeacon` on `pagehide`); each session's lines are
//! appended, in arrival order, to one file per session. That is the whole
//! protocol, so the HTTP is parsed by hand the way `emu serve`'s door
//! does, rather than pulling in a server framework.
//!
//! # Cross-origin, and Chrome's private-network preflight
//!
//! The page is usually NOT on this origin — prod Studio is
//! `https://lightplayer.app`, the dev server has its own port — so every
//! reply carries `Access-Control-Allow-Origin: *`, and an `OPTIONS`
//! preflight is answered with the methods and headers a POST needs plus
//! `Access-Control-Allow-Private-Network: true`: a public (https) page
//! reaching a loopback or LAN address is a Private Network Access request,
//! and Chrome asks first.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

/// The largest request head this receiver reads before giving up.
const MAX_HEAD: usize = 16 * 1024;
/// The largest body it accepts (a batch is a quarter second of a session;
/// a `pagehide` beacon is capped by the browser at 64 KiB).
const MAX_BODY: usize = 32 * 1024 * 1024;

/// Where sessions are written, and which file each one already has.
pub struct Recorder {
    out: PathBuf,
    sessions: Mutex<HashMap<String, PathBuf>>,
}

impl Recorder {
    pub fn new(out: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            out,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    /// Append one batch to its session's file, creating the file (and
    /// announcing the session on stdout) on the session's first batch.
    async fn ingest(&self, session: &str, body: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let path = match sessions.get(session) {
            Some(path) => path.clone(),
            None => {
                let path = session_file(&self.out, session);
                println!("session {session} → {}", path.display());
                sessions.insert(session.to_string(), path.clone());
                path
            }
        };
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(body).await?;
        if !body.is_empty() && !body.ends_with(b"\n") {
            file.write_all(b"\n").await?;
        }
        file.flush().await?;
        Ok(())
    }
}

/// The `?record=` query a Studio URL needs to stream to `sink`.
pub fn record_query(sink: &str) -> String {
    let mut encoded = String::new();
    for byte in sink.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("?record={encoded}")
}

/// `<out>/<YYYYMMDD-HHMMSS>-<session>.jsonl`, stamped in local time when
/// the session's first batch arrives.
fn session_file(out: &Path, session: &str) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    out.join(format!("{stamp}-{session}.jsonl"))
}

/// The session id a request names, reduced to a safe filename part:
/// `[A-Za-z0-9_-]`, at most 64 characters, `unknown` when absent.
fn session_of(query: &str) -> String {
    let raw = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "session")
        .map(|(_, value)| value)
        .unwrap_or_default();
    let safe: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if safe.is_empty() {
        "unknown".to_string()
    } else {
        safe
    }
}

/// Accept forever. One task per connection.
pub async fn run(listener: TcpListener, recorder: Arc<Recorder>) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let recorder = Arc::clone(&recorder);
                tokio::spawn(async move {
                    if let Err(e) = handle(stream, recorder).await {
                        log::debug!("record serve: {peer}: {e}");
                    }
                });
            }
            Err(e) => {
                eprintln!("record serve: accept failed: {e}");
                return;
            }
        }
    }
}

async fn handle(stream: TcpStream, recorder: Arc<Recorder>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    let head = read_head(&mut reader).await?;
    let mut request_line = head.lines().next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_string();
    let target = request_line.next().unwrap_or_default().to_string();
    let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));

    match (method.as_str(), path) {
        ("OPTIONS", _) => respond(reader.get_mut(), "204 No Content", PREFLIGHT_HEADERS).await,
        ("POST", "/ingest") => {
            let length: usize = header(&head, "content-length")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            if length > MAX_BODY {
                return respond(reader.get_mut(), "413 Payload Too Large", "").await;
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).await?;
            let session = session_of(query);
            match recorder.ingest(&session, &body).await {
                Ok(()) => respond(reader.get_mut(), "204 No Content", "").await,
                Err(e) => {
                    eprintln!("record serve: session {session}: {e}");
                    respond(reader.get_mut(), "500 Internal Server Error", "").await
                }
            }
        }
        _ => respond(reader.get_mut(), "404 Not Found", "").await,
    }
}

/// What an `OPTIONS` preflight is answered with (see the module doc).
const PREFLIGHT_HEADERS: &str = "Access-Control-Allow-Methods: POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: content-type\r\n\
     Access-Control-Allow-Private-Network: true\r\n\
     Access-Control-Max-Age: 600\r\n";

async fn respond(stream: &mut TcpStream, status: &str, extra_headers: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nAccess-Control-Allow-Origin: *\r\n{extra_headers}\
         Content-Length: 0\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_head(reader: &mut BufReader<TcpStream>) -> Result<String> {
    let mut head = String::new();
    loop {
        let before = head.len();
        let n = reader.read_line(&mut head).await?;
        if n == 0 {
            bail!("the client hung up before finishing its request");
        }
        if head.len() > MAX_HEAD {
            bail!("request head over {MAX_HEAD} bytes");
        }
        if head[before..] == *"\r\n" || head[before..] == *"\n" {
            return Ok(head);
        }
    }
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_is_a_safe_filename_part() {
        assert_eq!(session_of("session=0a1b2c3d"), "0a1b2c3d");
        assert_eq!(session_of("x=1&session=../../etc"), "etc");
        assert_eq!(session_of(""), "unknown");
        assert_eq!(session_of("session="), "unknown");
    }

    #[test]
    fn the_record_query_is_percent_encoded() {
        assert_eq!(
            record_query("http://127.0.0.1:4321/ingest"),
            "?record=http%3A%2F%2F127.0.0.1%3A4321%2Fingest"
        );
    }

    #[test]
    fn header_lookup_ignores_case() {
        let head = "POST /ingest HTTP/1.1\r\nContent-Length: 12\r\n\r\n";
        assert_eq!(header(head, "content-length"), Some("12"));
        assert_eq!(header(head, "origin"), None);
    }
}
