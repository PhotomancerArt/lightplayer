//! The LAN endpoint on `lp-net`: secure lp-link links at `ws://<board>/link`
//! (plan P04, MD10–MD12).
//!
//! One task per LAN slot ([`LAN_LINK_SLOTS`]: one on the C6), each holding
//! one listening TCP socket on port 80. A connection is upgraded to a
//! WebSocket (`fw_esp32_common::net::ws`, over the [`ByteStream`] a TLS
//! wrapper can later replace), then gets its own secure lp-link session on
//! the slot (`RadioLinkSlot::open_lan`: the board as the Noise responder,
//! keyed like Bluetooth by the server's access store). One binary message is
//! one lp-link frame. The mux on the main thread carries it like a Bluetooth
//! link (`radio_link::link_mux_transport`), under the port's lock.
//!
//! - **One connection too many** while every slot is busy reaches the
//!   [`refuse_task`]'s socket and gets WebSocket close 1013 ("try again
//!   later") and a log line.
//! - **Memory.** Each slot's TCP and WebSocket buffers are allocated once,
//!   never per connection. A board that boots with a network saved gets
//!   them at boot (`net_thread::start`'s [`super::net_thread::NetBuffers`]),
//!   low in the heap, before anything a link or a project leaves behind; a
//!   board that saves its first network later gets them at its first
//!   address, and one that never joins never pays (the 2026-09-24
//!   fragmentation class). The lp-link session itself is allocated per
//!   connection (`Link::new_secure`), as Bluetooth's is.
//! - **No plain link, ever.** Nothing reaches the server before the Noise
//!   handshake completes: the slot's link is secure from its first frame.
//! - A closed WebSocket, a TCP reset, a refused handshake that the peer
//!   gives up on, or the mux's close request ends the link with a logged
//!   reason; its session goes on the server's next tick.

use alloc::boxed::Box;
use alloc::vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::net_address;
use embassy_futures::select::{Either4, select4};
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use fw_esp32_common::net::ws::{ByteStream, CloseCode, RX_OVERHEAD, StreamClosed, WsConnection};
use fw_esp32_common::radio_link::lan_link_config::LAN_MAX_FRAME;
use fw_esp32_common::radio_link::{
    LAN_LINK_SLOTS, RADIO_LINK_SLOTS, RadioLinkEvent, SharedPort, now_us,
};

/// The endpoint's port (plan Q6: 80, room for the device-served panel).
pub const LINK_PORT: u16 = 80;
/// TCP buffers per link. Receive: two WebSocket frames. Send: a whole
/// lp-link window (2 frames of `LAN_MAX_FRAME` plus their WebSocket
/// headers) and room for the ACKs and a keepalive beside it, so the link
/// never waits on its own socket for a window it may send.
const TCP_RX: usize = 2 * 1024;
const TCP_TX: usize = 4 * 1024;
/// A server frame's WebSocket header at the sizes a link sends (126..65535
/// bytes: 4 B; shorter: 2 B).
const WS_TX_HEADER: usize = 4;
const _: () = assert!(2 * (LAN_MAX_FRAME + WS_TX_HEADER) + 256 <= TCP_TX);
/// A peer that sends nothing for this long is gone (lp-link's own keepalive
/// is 1 s, so a live peer never gets near it).
const IDLE_TIMEOUT: Duration = Duration::from_secs(20);
/// How often a link with nothing to send still writes its `[lan]` line.
const STATS_EVERY: Duration = Duration::from_secs(10);
/// WebSocket close 1013: try again later.
const TRY_AGAIN_LATER: CloseCode = CloseCode(1013);

/// LAN slots holding a link.
static BUSY: AtomicUsize = AtomicUsize::new(0);
/// The busy count changed.
static BUSY_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// One slot's buffers, allocated once (on the heap directly: a 5 KB value
/// built on `lp-net`'s stack first would be most of it).
pub struct LanBuffers {
    tcp_rx: &'static mut [u8],
    tcp_tx: &'static mut [u8],
    ws_rx: &'static mut [u8],
    frame_tx: &'static mut [u8],
}

impl LanBuffers {
    pub fn leak() -> Self {
        let leak =
            |len: usize| -> &'static mut [u8] { Box::leak(vec![0u8; len].into_boxed_slice()) };
        Self {
            tcp_rx: leak(TCP_RX),
            tcp_tx: leak(TCP_TX),
            ws_rx: leak(LAN_MAX_FRAME + RX_OVERHEAD),
            frame_tx: leak(LAN_MAX_FRAME),
        }
    }
}

/// One LAN slot's task: `lan` is its index among the LAN slots.
#[embassy_executor::task(pool_size = LAN_LINK_SLOTS)]
pub async fn lan_link_task(
    stack: Stack<'static>,
    port: SharedPort,
    lan: usize,
    buffers: Option<LanBuffers>,
) {
    let mut address = net_address::watch();
    let buffers = match buffers {
        Some(buffers) => buffers,
        None => {
            net_address::wait_up(&mut address).await;
            LanBuffers::leak()
        }
    };
    let index = RADIO_LINK_SLOTS + lan;
    loop {
        net_address::wait_up(&mut address).await;
        let mut socket = TcpSocket::new(stack, &mut *buffers.tcp_rx, &mut *buffers.tcp_tx);
        socket.set_timeout(Some(IDLE_TIMEOUT));
        // One write is one whole WebSocket message (one lp-link frame): send
        // it now. With Nagle on, a short frame waited for the peer's delayed
        // ACK of the one before (40 ms or more on a real host), which the
        // emulated walk measured as resends and an editor would feel as lag.
        socket.set_nagle_enabled(false);
        if socket.accept(LINK_PORT).await.is_err() {
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }
        let peer = socket.remote_endpoint();
        let ws = match WsConnection::accept(TcpStream(socket), &mut *buffers.ws_rx).await {
            Ok(ws) => ws,
            Err(_) => {
                log::info!("[lan] a request on port {LINK_PORT} was not a link upgrade");
                continue;
            }
        };
        set_busy(1);
        let slot = port.slot(index);
        let id = port.mint_link();
        slot.reset();
        let payload = slot.open_lan(id, random_u32(), fill_random);
        match peer {
            Some(peer) => log::info!(
                "[lan] link {id} from {peer}: secure session opening ({payload} B frames)"
            ),
            None => log::info!("[lan] link {id}: secure session opening ({payload} B frames)"),
        }
        port.announce(RadioLinkEvent::Opened {
            link: id,
            slot: index,
        })
        .await;
        let reason = serve(ws, port, index, &mut *buffers.frame_tx).await;
        log_counters(port, index, id);
        slot.close();
        port.announce(RadioLinkEvent::Closed { link: id }).await;
        log::info!("[lan] link {id}: closed ({reason})");
        set_busy(-1);
    }
}

/// Carry frames between the WebSocket and the slot's link until either
/// side ends it; why it ended.
async fn serve(
    mut ws: WsConnection<'_, TcpStream<'_>>,
    port: SharedPort,
    index: usize,
    frame_tx: &mut [u8],
) -> &'static str {
    let slot = port.slot(index);
    let mut next_stats = Instant::now() + STATS_EVERY;
    loop {
        // Everything the link wants to send now, one message per frame.
        while let Some(len) = slot.poll_frame(now_us(), |frame| {
            let n = frame.len().min(frame_tx.len());
            frame_tx[..n].copy_from_slice(&frame[..n]);
            n
        }) {
            if ws.send(&frame_tx[..len]).await.is_err() {
                return "the peer stopped taking frames";
            }
        }
        let wake = slot
            .poll_timeout()
            .map(|at| Instant::from_micros(at).min(next_stats))
            .unwrap_or(next_stats);
        match select4(
            ws.recv(),
            slot.doorbell(),
            Timer::at(wake),
            slot.close_requested(),
        )
        .await
        {
            Either4::First(Ok(frame)) => slot.on_datagram(now_us(), frame),
            Either4::First(Err(_)) => return "the WebSocket closed",
            Either4::Second(()) => {}
            Either4::Third(()) => {
                if Instant::now() >= next_stats {
                    next_stats = Instant::now() + STATS_EVERY;
                    if let Some(id) = slot_link_id(port, index) {
                        log_counters(port, index, id);
                    }
                }
            }
            Either4::Fourth(reason) => {
                ws.close(CloseCode::NORMAL).await;
                return reason;
            }
        }
    }
}

/// The refuser's buffers: enough to read an upgrade request and close.
pub struct RefuseBuffers {
    tcp_rx: &'static mut [u8],
    tcp_tx: &'static mut [u8],
    ws_rx: &'static mut [u8],
}

impl RefuseBuffers {
    pub fn leak() -> Self {
        let leak =
            |len: usize| -> &'static mut [u8] { Box::leak(vec![0u8; len].into_boxed_slice()) };
        Self {
            tcp_rx: leak(512),
            tcp_tx: leak(256),
            ws_rx: leak(1024 + RX_OVERHEAD),
        }
    }
}

/// Answer a connection that finds every LAN slot busy: WebSocket close 1013
/// ("try again later"). Its socket listens only while all are busy.
#[embassy_executor::task]
pub async fn refuse_task(stack: Stack<'static>, buffers: Option<RefuseBuffers>) {
    let RefuseBuffers {
        tcp_rx,
        tcp_tx,
        ws_rx,
    } = match buffers {
        Some(buffers) => buffers,
        None => {
            let mut address = net_address::watch();
            net_address::wait_up(&mut address).await;
            RefuseBuffers::leak()
        }
    };
    loop {
        while BUSY.load(Ordering::Acquire) < LAN_LINK_SLOTS {
            BUSY_CHANGED.wait().await;
        }
        let mut socket = TcpSocket::new(stack, &mut *tcp_rx, &mut *tcp_tx);
        socket.set_timeout(Some(Duration::from_secs(5)));
        if socket.accept(LINK_PORT).await.is_err() {
            continue;
        }
        if let Ok(ws) = WsConnection::accept(TcpStream(socket), &mut *ws_rx).await {
            log::warn!("[lan] every LAN link is in use: a new one was told to try again later");
            ws.close(TRY_AGAIN_LATER).await;
        }
    }
}

/// A TCP socket as the WebSocket's byte stream.
pub struct TcpStream<'a>(TcpSocket<'a>);

impl ByteStream for TcpStream<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, StreamClosed> {
        self.0.read(buf).await.map_err(|_| StreamClosed)
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), StreamClosed> {
        while !buf.is_empty() {
            match self.0.write(buf).await {
                Ok(0) | Err(_) => return Err(StreamClosed),
                Ok(n) => buf = &buf[n..],
            }
        }
        Ok(())
    }

    async fn close(&mut self) {
        let _ = self.0.flush().await;
        self.0.close();
        let _ = self.0.flush().await;
    }
}

/// One `[lan]` line: the link's frames and its secure counters (Display;
/// never a key or a payload).
fn log_counters(port: SharedPort, index: usize, id: lpc_shared::transport::LinkId) {
    if let Some(c) = port.slot(index).counters() {
        log::info!(
            "[lan] link {id}: frames in {} out {} · handshakes {} refused {} · seal failures {} \
             replays {} · resends {}",
            c.frames_rx,
            c.frames_tx,
            c.handshakes,
            c.handshake_refusals,
            c.seal_failures,
            c.replays,
            c.retransmits
        );
    }
}

fn slot_link_id(port: SharedPort, index: usize) -> Option<lpc_shared::transport::LinkId> {
    port.slot(index).link_id()
}

fn set_busy(delta: isize) {
    if delta > 0 {
        BUSY.fetch_add(1, Ordering::AcqRel);
    } else {
        BUSY.fetch_sub(1, Ordering::AcqRel);
    }
    BUSY_CHANGED.signal(());
}

fn fill_random(buf: &mut [u8]) {
    esp_hal::rng::Rng::new().read(buf);
}

fn random_u32() -> u32 {
    esp_hal::rng::Rng::new().random()
}
