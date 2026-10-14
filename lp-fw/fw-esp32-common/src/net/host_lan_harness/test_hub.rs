//! A stand-in relay hub for this crate's tests: the device leg's half of
//! `lp-cloud-server`'s hub, cut to what a board-side test needs. It takes a
//! board's leg on `/relay/device`, registers it (any proof), and lets the
//! test act as browsers: open a route, pass lp-link frames on it, and see
//! what the board sends and closes. The real hub's own rules are proven in
//! `lp-cloud-server`, and the board against it end to end in
//! `lp-cli/tests/relay_link.rs`.

extern crate std;

use alloc::vec::Vec;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use lpc_relay::{PictureRate, RelayFrame, RelayPicture, RelayProject, RouteCloseReason};
use tungstenite::{Message, WebSocket};

/// What the board did, as the hub saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubEvent {
    /// A board registered, naming this many accounts.
    Registered { accounts: usize },
    /// A frame for a browser on `route`.
    Frame { route: u16, bytes: Vec<u8> },
    /// The board closed `route`.
    Closed {
        route: u16,
        reason: RouteCloseReason,
    },
    /// The device leg ended.
    LegClosed,
    /// The board's picture (relay protocol 2).
    Picture(RelayPicture),
    /// The board's project report (relay protocol 2).
    Project(Option<RelayProject>),
}

enum HubCommand {
    Send(RelayFrame),
    /// Close the board's device leg (the board dials again).
    DropLeg,
    Stop,
}

/// See the module doc.
pub struct TestHub {
    pub port: u16,
    commands: Sender<HubCommand>,
    events: Receiver<HubEvent>,
    thread: Option<JoinHandle<()>>,
    /// Frames for routes the test has not asked about yet.
    pending: Vec<HubEvent>,
}

impl TestHub {
    /// Listen on `127.0.0.1:0` and serve device legs, one at a time.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (commands, command_rx) = channel();
        let (event_tx, events) = channel();
        let thread = std::thread::spawn(move || serve(&listener, &command_rx, &event_tx));
        Self {
            port,
            commands,
            events,
            thread: Some(thread),
            pending: Vec::new(),
        }
    }

    /// Open `route`, as a browser arriving.
    pub fn open(&self, route: u16) {
        self.send(RelayFrame::Open { route });
    }

    /// One lp-link frame from the browser on `route`.
    pub fn frame(&self, route: u16, bytes: &[u8]) {
        self.send(RelayFrame::Frame {
            route,
            bytes: bytes.to_vec(),
        });
    }

    /// The browser on `route` left.
    pub fn close(&self, route: u16) {
        self.send(RelayFrame::Close {
            route,
            reason: RouteCloseReason::Gone,
        });
    }

    /// Close the board's device leg, as a hub that lost it would.
    pub fn drop_leg(&self) {
        let _ = self.commands.send(HubCommand::DropLeg);
    }

    /// Tell the board how fast to send pictures, as the real hub does after
    /// `Registered` and while someone watches.
    pub fn send_rate(&self, rate: PictureRate) {
        self.send(RelayFrame::PictureRate(rate));
    }

    fn send(&self, frame: RelayFrame) {
        let _ = self.commands.send(HubCommand::Send(frame));
    }

    /// Wait up to `wait` for an event `want` matches, keeping the others.
    pub fn wait_for(
        &mut self,
        wait: Duration,
        want: impl Fn(&HubEvent) -> bool,
    ) -> Option<HubEvent> {
        if let Some(at) = self.pending.iter().position(&want) {
            return Some(self.pending.remove(at));
        }
        let until = Instant::now() + wait;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(event) if want(&event) => return Some(event),
                Ok(event) => self.pending.push(event),
                Err(_) => return None,
            }
        }
        None
    }

    /// The frames the board sent on `route` so far, taken.
    pub fn take_frames(&mut self, route: u16) -> Vec<Vec<u8>> {
        while let Ok(event) = self.events.try_recv() {
            self.pending.push(event);
        }
        let mut frames = Vec::new();
        self.pending.retain(|event| match event {
            HubEvent::Frame { route: r, bytes } if *r == route => {
                frames.push(bytes.clone());
                false
            }
            _ => true,
        });
        frames
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        let _ = self.commands.send(HubCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(listener: &TcpListener, commands: &Receiver<HubCommand>, events: &Sender<HubEvent>) {
    loop {
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(_) => {
                    if matches!(commands.try_recv(), Ok(HubCommand::Stop)) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        };
        socket.set_nonblocking(false).unwrap();
        let Ok(mut ws) = tungstenite::accept(socket) else {
            continue;
        };
        let _ = ws.get_mut().set_nodelay(true);
        ws.get_mut()
            .set_read_timeout(Some(Duration::from_millis(2)))
            .unwrap();
        if !serve_leg(&mut ws, commands, events) {
            return;
        }
        let _ = events.send(HubEvent::LegClosed);
    }
}

/// One device leg; `false` when the hub is stopping.
fn serve_leg(
    ws: &mut WebSocket<TcpStream>,
    commands: &Receiver<HubCommand>,
    events: &Sender<HubEvent>,
) -> bool {
    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                HubCommand::Send(frame) => {
                    if ws.send(Message::binary(frame.encode())).is_err() {
                        return true;
                    }
                }
                HubCommand::DropLeg => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return true;
                }
                HubCommand::Stop => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return false;
                }
            }
        }
        match ws.read() {
            Ok(Message::Binary(bytes)) => {
                let reply = match RelayFrame::decode(&bytes) {
                    Ok(RelayFrame::Hello(hello)) => {
                        let _ = events.send(HubEvent::Registered {
                            accounts: hello.accounts.len(),
                        });
                        Some(RelayFrame::Challenge { nonce: [1; 32] })
                    }
                    Ok(RelayFrame::Proof { .. }) => Some(RelayFrame::Registered {
                        accounts_ok: 0xff,
                        ping_s: 25,
                    }),
                    Ok(RelayFrame::Frame { route, bytes }) => {
                        let _ = events.send(HubEvent::Frame { route, bytes });
                        None
                    }
                    Ok(RelayFrame::Close { route, reason }) => {
                        let _ = events.send(HubEvent::Closed { route, reason });
                        None
                    }
                    Ok(RelayFrame::Picture(picture)) => {
                        let _ = events.send(HubEvent::Picture(picture));
                        None
                    }
                    Ok(RelayFrame::Project(project)) => {
                        let _ = events.send(HubEvent::Project(project));
                        None
                    }
                    _ => None,
                };
                if let Some(reply) = reply
                    && ws.send(Message::binary(reply.encode())).is_err()
                {
                    return true;
                }
            }
            Ok(Message::Close(_)) => return true,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return true,
        }
    }
}
