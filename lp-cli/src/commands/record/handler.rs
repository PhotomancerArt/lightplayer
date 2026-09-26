//! `lp-cli record …`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use anyhow::{Context, Result};

use super::args::{RecordCli, RecordSubcommand, ServeArgs};
use super::serve::{Recorder, record_query};

/// Run `lp-cli record …`.
pub fn handle_record(cli: RecordCli) -> Result<()> {
    match cli.subcommand {
        RecordSubcommand::Serve(args) => handle_serve(&args),
        RecordSubcommand::Timeline(args) => super::timeline::handle_timeline(&args),
    }
}

fn handle_serve(args: &ServeArgs) -> Result<()> {
    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the tokio runtime")?;
    runtime.block_on(async {
        let bind = if args.lan {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        let listener = tokio::net::TcpListener::bind(SocketAddr::new(bind, args.port))
            .await
            .with_context(|| format!("binding {bind}:{}", args.port))?;
        let port = listener.local_addr()?.port();
        let host = if args.lan {
            lan_address().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
        } else {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        };
        let sink = format!("http://{host}:{port}/ingest");
        println!("recording Studio sessions into {}", args.out.display());
        println!("  sink     {sink}");
        println!("  add to a Studio URL:  {}", record_query(&sink));
        if args.lan {
            println!(
                "  (listening on every interface; loopback is http://127.0.0.1:{port}/ingest)"
            );
        }
        println!("  the recording is unredacted: device traffic, access handshakes, projects");
        let recorder = Recorder::new(args.out.clone());
        super::serve::run(listener, recorder).await;
        Ok(())
    })
}

/// This machine's address on the LAN: the local end of a UDP socket
/// "connected" to a public address (connecting a UDP socket sends
/// nothing; it only picks the route).
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip())
}
