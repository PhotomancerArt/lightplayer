//! `lp-cli link …`: the soak reader's IO edge (the checking is
//! [`super::soak_verifier`]'s).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::args::{LinkCli, LinkSubcommand, SoakArgs, SoakVerifyArgs};
use super::soak_text::echo_text;
use super::soak_verifier::SoakVerifier;

/// Run `lp-cli link …`.
pub fn handle_link(cli: LinkCli) -> Result<()> {
    match cli.subcommand {
        LinkSubcommand::Soak(args) => soak(&args),
        LinkSubcommand::SoakVerify(args) => soak_verify(&args),
        LinkSubcommand::Lab(args) => super::lab_cmd::lab(&args),
    }
}

/// A port the reader holds: a serial device or an emulated board's socket.
enum Port {
    #[cfg(unix)]
    Serial(serialport::TTYPort),
    Tcp(TcpStream),
}

impl Port {
    fn open(spec: &str) -> Result<Self> {
        if let Some(addr) = spec.strip_prefix("tcp://") {
            let s = TcpStream::connect(addr).with_context(|| format!("connect {addr}"))?;
            s.set_read_timeout(Some(Duration::from_millis(10)))?;
            s.set_nodelay(true)?;
            return Ok(Port::Tcp(s));
        }
        #[cfg(unix)]
        {
            // The transport's own settings (lpa-client serialport_stream):
            // raw 8N1, no flow control; nothing toggles DTR/RTS here.
            let port = serialport::new(spec, lpc_model::DEFAULT_SERIAL_BAUD_RATE)
                .data_bits(serialport::DataBits::Eight)
                .stop_bits(serialport::StopBits::One)
                .parity(serialport::Parity::None)
                .flow_control(serialport::FlowControl::None)
                .timeout(Duration::from_millis(10))
                .open_native()
                .with_context(|| format!("open serial port {spec}"))?;
            Ok(Port::Serial(port))
        }
        #[cfg(not(unix))]
        bail!("serial ports need a unix host here; use tcp://")
    }

    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let r = match self {
            #[cfg(unix)]
            Port::Serial(p) => p.read(buf),
            Port::Tcp(s) => s.read(buf),
        };
        match r {
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(0)
            }
            Ok(0) if matches!(self, Port::Tcp(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the link closed",
            )),
            other => other,
        }
    }

    /// Write everything, riding out the short read timeout the port is
    /// opened with (a board busy writing takes its host bytes late); gives up
    /// after five seconds without progress.
    fn write_all(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        let mut stalled_since = Instant::now();
        while !bytes.is_empty() {
            let r = match self {
                #[cfg(unix)]
                Port::Serial(p) => p.write(bytes),
                Port::Tcp(s) => s.write(bytes),
            };
            match r {
                Ok(n) if n > 0 => {
                    bytes = &bytes[n..];
                    stalled_since = Instant::now();
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(e),
            }
            if stalled_since.elapsed() > Duration::from_secs(5) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the board took no host bytes for 5 s",
                ));
            }
        }
        Ok(())
    }
}

fn soak(args: &SoakArgs) -> Result<()> {
    let packed = match args.encoding.as_str() {
        "packed" => true,
        "json" => false,
        other => bail!("--encoding must be packed or json, not {other}"),
    };
    let mut port = Port::open(&args.port)?;
    let mut verifier = SoakVerifier::new(packed);
    let mut capture = Vec::new();
    let mut events = Vec::new();
    let mut buf = vec![0u8; args.read_size.max(1)];
    let clock = Instant::now();
    let ms = |c: &Instant| c.elapsed().as_millis() as u64;

    // Ask for a hello: its answer is what tells the opt-in the board packs.
    let hello = lpc_wire::json::to_serial_line(&lpc_wire::message::client::ClientMessage {
        id: 1,
        msg: lpc_wire::message::client::ClientRequest::Hello,
    })?;
    port.write_all(hello.as_bytes())?;
    if !packed {
        // A board keeps a link's packed agreement until the link resets, so a
        // JSON soak right after a packed one must ask for JSON outright.
        let json_please =
            lpc_wire::json::to_serial_line(&lpc_wire::message::client::ClientMessage {
                id: 2,
                msg: lpc_wire::message::client::ClientRequest::SetEncoding {
                    encoding: lpc_wire::WireEncoding::Json,
                    format: lpc_wire::PACK_FORMAT_VERSION,
                },
            })?;
        port.write_all(json_please.as_bytes())?;
    }

    // The encoding settles first (the hello's answer, then the opt-in), with
    // no soak running; then the soak runs for --seconds; then it stops and
    // the tail is read (2.5 s, so the last stat arrives).
    let soak_start = if packed { 1_500 } else { 500 };
    let soak_end = soak_start + (args.seconds * 1000.0) as u64;
    let tail_end = soak_end + 2_500;
    let mut started = false;
    let mut stopped = false;
    let mut next_stall = args.stall_every_ms;
    let mut echo_seq = 0u32;
    let mut echo_bytes_sent = 0u64;
    let mut stalls = 0u64;
    let mut stall_ms_total = 0u64;

    loop {
        let now = ms(&clock);
        if now >= tail_end {
            break;
        }
        if !started && now >= soak_start {
            let line = format!(
                "\nSOAK! on=1 min={} max={} rate={} logs={} seed={} budget={} count=0\n",
                args.min, args.max, args.rate, args.logs, args.seed, args.budget
            );
            port.write_all(line.as_bytes())?;
            verifier.restart_sequence();
            started = true;
        }
        if started && !stopped && now >= soak_end {
            port.write_all(b"\nSOAK! on=0\n")?;
            stopped = true;
        }
        // A stall: the application stops reading; the OS keeps what it can.
        if started
            && !stopped
            && args.stall_every_ms > 0
            && args.stall_ms > 0
            && now >= soak_start + next_stall
        {
            std::thread::sleep(Duration::from_millis(args.stall_ms));
            stalls += 1;
            stall_ms_total += args.stall_ms;
            next_stall += args.stall_every_ms;
            events.push(
                json!({"kind": "reader-stall", "at_ms": now, "ms": args.stall_ms,
                "offset": verifier.tally().bytes}),
            );
        }
        // Host → board echo, paced to --echo-bps.
        if started && !stopped && args.echo_bps > 0 {
            let due = (now - soak_start) * u64::from(args.echo_bps) / 1000;
            while echo_bytes_sent < due {
                let line = format!("\nSOAK> {}\n", &echo_text(echo_seq, args.echo_len)[5..]);
                port.write_all(line.as_bytes())?;
                echo_bytes_sent += line.len() as u64;
                echo_seq += 1;
            }
        }
        let n = port.read(&mut buf).context("reading the port")?;
        if n > 0 {
            capture.extend_from_slice(&buf[..n]);
            verifier.push(&buf[..n], ms(&clock));
            for line in verifier.take_to_send() {
                port.write_all(line.as_bytes())?;
            }
            events.extend(verifier.take_events());
        }
    }

    let tally = verifier.tally().clone();
    let summary = json!({
        "label": args.label,
        "port": args.port,
        "encoding": args.encoding,
        "seconds": args.seconds,
        "min": args.min, "max": args.max, "rate": args.rate, "logs": args.logs,
        "stall_every_ms": args.stall_every_ms, "stall_ms": args.stall_ms,
        "stalls": stalls, "stall_ms_total": stall_ms_total,
        "echo_bps": args.echo_bps, "echo_len": args.echo_len, "echo_sent": echo_seq,
        "read_size": args.read_size,
        "clean": tally.clean(),
        "tally": tally.to_json(),
    });
    println!("{}", tally.summary());
    if args.echo_bps > 0 {
        println!("echo: sent {echo_seq} lines");
    }
    write_outputs(args.out.as_deref(), Some(&capture), &events, &summary)?;
    Ok(())
}

fn soak_verify(args: &SoakVerifyArgs) -> Result<()> {
    let bytes = std::fs::read(&args.capture)
        .with_context(|| format!("reading {}", args.capture.display()))?;
    let mut verifier = SoakVerifier::new(false);
    verifier.push(&bytes, 0);
    let events = verifier.take_events();
    let tally = verifier.tally().clone();
    let summary = json!({
        "label": args.label,
        "capture": args.capture,
        "clean": tally.clean(),
        "tally": tally.to_json(),
    });
    println!("{}", tally.summary());
    write_outputs(args.out.as_deref(), None, &events, &summary)
}

fn write_outputs(
    out: Option<&Path>,
    capture: Option<&[u8]>,
    events: &[serde_json::Value],
    summary: &serde_json::Value,
) -> Result<()> {
    let Some(dir) = out else {
        return Ok(());
    };
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    if let Some(capture) = capture {
        std::fs::write(dir.join("capture.bin"), capture)?;
    }
    let mut lines = String::new();
    for e in events {
        lines.push_str(&e.to_string());
        lines.push('\n');
    }
    std::fs::write(dir.join("events.jsonl"), lines)?;
    std::fs::write(
        dir.join("summary.json"),
        serde_json::to_string_pretty(summary)?,
    )?;
    eprintln!("wrote {}", dir.display());
    Ok(())
}
