//! `lp-cli lan list`: browse DNS-SD `_lightplayer._tcp.local` on this host and
//! print the boards that answer.
//!
//! One plain `UdpSocket` on an ephemeral port sends the PTR question to the
//! mDNS group. A source port other than 5353 makes it an RFC 6762 §6.7 legacy
//! unicast query, so each board answers straight to this socket and the host's
//! own mDNS responder is never involved. The question goes out at the start
//! and again halfway through the wait, in case one packet was lost.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use super::args::{LanCli, LanCommand, ListArgs};
use super::lan_answer::{LanBoard, build_query, merge_boards, parse_answer};

/// The mDNS group and port (RFC 6762 §3).
const MDNS_GROUP: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)), 5353);

/// The longest a browse may listen: longer is a typo, not a wish.
const MAX_WAIT_SECS: f64 = 60.0;

pub fn handle_lan(cli: LanCli) -> Result<()> {
    match cli.command {
        LanCommand::List(args) => list(&args),
    }
}

fn list(args: &ListArgs) -> Result<()> {
    if !args.wait.is_finite() || args.wait <= 0.0 || args.wait > MAX_WAIT_SECS {
        bail!("--wait must be more than 0 and at most {MAX_WAIT_SECS} seconds");
    }
    let boards = browse(MDNS_GROUP, Duration::from_secs_f64(args.wait))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&boards)?);
    } else {
        for line in board_lines(&boards) {
            println!("{line}");
        }
    }
    if boards.is_empty() {
        eprintln!(
            "No LightPlayer boards answered in {} s. A board answers only once it has \
             joined this network's Wi-Fi; check that this machine is on the same network.",
            args.wait
        );
    }
    Ok(())
}

/// Ask `target` (the mDNS group, outside tests) which boards are there and
/// collect answers for `wait`.
fn browse(target: SocketAddr, wait: Duration) -> Result<Vec<LanBoard>> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).context("opening a UDP socket")?;
    // Link-local multicast stays on the link either way; 255 is what mDNS asks.
    let _ = socket.set_multicast_ttl_v4(255);

    let query = build_query(query_id());
    let start = Instant::now();
    let deadline = start + wait;
    let resend_at = start + wait / 2;
    let mut resent = false;
    let mut boards = Vec::new();
    let mut buffer = [0u8; 2048];

    socket
        .send_to(&query, target)
        .with_context(|| format!("sending the mDNS question to {target}"))?;

    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        if !resent && now >= resend_at {
            resent = true;
            // A lost resend is no worse than the first having been lost.
            let _ = socket.send_to(&query, target);
        }
        let next_stop = if resent {
            deadline
        } else {
            resend_at.min(deadline)
        };
        let slice = next_stop
            .saturating_duration_since(now)
            .max(Duration::from_millis(1));
        socket.set_read_timeout(Some(slice))?;
        match socket.recv_from(&mut buffer) {
            Ok((len, _from)) => merge_boards(&mut boards, parse_answer(&buffer[..len])),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            // Some hosts report a refused earlier send as the next read's error.
            Err(e) if e.kind() == ErrorKind::ConnectionReset => {}
            Err(e) => return Err(e).context("reading mDNS answers"),
        }
    }
    Ok(boards)
}

/// One line per board, columns aligned: name, address and port, MAC, wire
/// version, instance, and the `lan:` specifier to use.
pub fn board_lines(boards: &[LanBoard]) -> Vec<String> {
    let rows: Vec<[String; 6]> = boards
        .iter()
        .map(|board| {
            [
                board.name.clone(),
                format!("{}:{}", board.ip, board.port),
                board.mac.clone().unwrap_or_else(|| "-".into()),
                board
                    .proto
                    .map_or_else(|| "proto -".into(), |proto| format!("proto {proto}")),
                format!("\"{}\"", board.instance),
                board.spec.clone(),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..6)
        .map(|column| rows.iter().map(|row| row[column].len()).max().unwrap_or(0))
        .collect();
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (column, cell) in row.iter().enumerate() {
                if column > 0 {
                    line.push_str("  ");
                }
                if column + 1 == row.len() {
                    line.push_str(cell);
                } else {
                    line.push_str(&format!("{cell:<width$}", width = widths[column]));
                }
            }
            line
        })
        .collect()
}

/// A non-zero id for the question (zero is a multicast answer's id, so a
/// non-zero one marks the legacy unicast ask). The clock is this edge's to
/// read; the id is only a label.
fn query_id() -> u16 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |since| since.subsec_nanos());
    ((nanos ^ (nanos >> 16)) as u16).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fw_esp32_common::net::mdns::{MdnsIdentity, build_answer, parse_query};

    #[test]
    fn a_browse_hears_a_board_that_answers_the_question() {
        let (responder_addr, responder) = fake_board(1);

        let boards = browse(responder_addr, Duration::from_millis(600)).unwrap();
        responder.join().unwrap();

        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].spec, "lan:192.168.4.100");
        assert_eq!(boards[0].instance, "Porch sign");
    }

    #[test]
    fn a_browse_with_no_answer_is_an_empty_list() {
        // A bound socket nobody reads: the question goes nowhere.
        let silent = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let target = silent.local_addr().unwrap();

        let boards = browse(target, Duration::from_millis(200)).unwrap();

        assert!(boards.is_empty());
    }

    #[test]
    fn a_browse_answered_twice_still_lists_one_board() {
        let (responder_addr, responder) = fake_board(2);

        let boards = browse(responder_addr, Duration::from_millis(600)).unwrap();
        responder.join().unwrap();

        assert_eq!(boards.len(), 1);
    }

    #[test]
    fn lines_read_in_columns_and_name_the_spec() {
        let boards = vec![
            LanBoard {
                name: "lp-8e30".into(),
                instance: "Porch sign".into(),
                ip: Ipv4Addr::new(192, 168, 4, 100),
                port: 80,
                mac: Some("10bda3b08e30".into()),
                proto: Some(33),
                path: Some("/link".into()),
                spec: "lan:192.168.4.100".into(),
            },
            LanBoard {
                name: "lp-0001".into(),
                instance: "Desk".into(),
                ip: Ipv4Addr::new(10, 0, 0, 7),
                port: 8080,
                mac: None,
                proto: None,
                path: None,
                spec: "lan:10.0.0.7:8080".into(),
            },
        ];

        assert_eq!(
            board_lines(&boards),
            vec![
                "lp-8e30  192.168.4.100:80  10bda3b08e30  proto 33  \"Porch sign\"  lan:192.168.4.100",
                "lp-0001  10.0.0.7:8080     -             proto -   \"Desk\"        lan:10.0.0.7:8080",
            ]
        );
        assert!(board_lines(&[]).is_empty());
    }

    #[test]
    fn the_query_id_is_never_zero() {
        assert_ne!(query_id(), 0);
    }

    /// A loopback "board": answers each question it gets with the firmware's
    /// own legacy-unicast answer, up to `answers` times, then returns.
    fn fake_board(answers: usize) -> (SocketAddr, std::thread::JoinHandle<()>) {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let addr = socket.local_addr().unwrap();
        let identity = MdnsIdentity {
            label: "lp-8e30".into(),
            instance: "Porch sign".into(),
            mac: [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30],
            proto: 33,
            port: 80,
            ipv4: [192, 168, 4, 100],
        };
        let handle = std::thread::spawn(move || {
            let mut buffer = [0u8; 512];
            let (len, from) = socket.recv_from(&mut buffer).unwrap();
            let query = parse_query(&buffer[..len], &identity.label, &identity.instance).unwrap();
            let mut out = [0u8; 512];
            let n = build_answer(&mut out, &identity, &query, Some(query.id)).unwrap();
            for _ in 0..answers {
                socket.send_to(&out[..n], from).unwrap();
            }
        });
        (addr, handle)
    }
}
