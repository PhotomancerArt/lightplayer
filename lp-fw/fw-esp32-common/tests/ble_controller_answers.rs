//! The host side of the C6's HCI against the answers its controller gives.
//!
//! The C6's BLE host (`fw-esp32c6::ble`) is trouble-host over bt-hci's
//! `ExternalController`, both forked in `third_party/` for this reason:
//! an Android central asks for connection parameters over the link layer,
//! the host answers with LE Remote Connection Parameter Request (Negative)
//! Reply, and the controller's Command Complete for either carries the
//! connection handle the spec gives them — 2 return bytes the command, as
//! bt-hci declares it, has no room for. Upstream sliced past the empty
//! buffer and the board panicked on every Android connection
//! (`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).

use core::cell::RefCell;
use core::future::{Future, poll_fn};
use core::task::Poll;
use std::collections::VecDeque;

use bt_hci::cmd::le::{
    LeRemoteConnectionParameterRequestNegativeReply, LeRemoteConnectionParameterRequestReply,
};
use bt_hci::controller::{ControllerCmdAsync, ExternalController};
use bt_hci::param::{ConnHandle, Duration, RemoteConnectionParamsRejectReason};
use bt_hci::transport::Transport;
use bt_hci::{ControllerToHostPacket, FromHciBytes, FromHciBytesError, HostToControllerPacket};
use embassy_futures::select::{Either, select};

#[test]
fn a_negative_reply_completed_with_its_handle_does_not_panic() {
    // Command Complete: 1 packet, opcode 0x2021, status success, handle 0.
    let answer = [0x04, 0x0E, 0x06, 0x01, 0x21, 0x20, 0x00, 0x00, 0x00];
    let cmd = LeRemoteConnectionParameterRequestNegativeReply::new(
        ConnHandle::new(0),
        RemoteConnectionParamsRejectReason::UnacceptableConnParameters,
    );
    assert_eq!(run(&cmd, &answer), Ok(()));
}

#[test]
fn a_reply_completed_with_its_handle_does_not_panic() {
    // Command Complete: 1 packet, opcode 0x2020, status success, handle 1.
    let answer = [0x04, 0x0E, 0x06, 0x01, 0x20, 0x20, 0x00, 0x01, 0x00];
    let cmd = LeRemoteConnectionParameterRequestReply::new(
        ConnHandle::new(1),
        Duration::from_u16(6),
        Duration::from_u16(6),
        0,
        Duration::from_u16(500),
        Duration::from_u16(0),
        Duration::from_u16(0),
    );
    assert_eq!(run(&cmd, &answer), Ok(()));
}

#[test]
fn a_refused_reply_still_says_so() {
    // Command Complete: opcode 0x2020, status 0x12 (invalid parameters), handle 0.
    let answer = [0x04, 0x0E, 0x06, 0x01, 0x20, 0x20, 0x12, 0x00, 0x00];
    let cmd = LeRemoteConnectionParameterRequestReply::new(
        ConnHandle::new(0),
        Duration::from_u16(6),
        Duration::from_u16(6),
        0,
        Duration::from_u16(500),
        Duration::from_u16(0),
        Duration::from_u16(0),
    );
    assert_eq!(run(&cmd, &answer), Err(0x12));
}

#[test]
fn a_command_status_answer_completes_it_too() {
    // Command Status: status success, 1 packet, opcode 0x2021.
    let answer = [0x04, 0x0F, 0x04, 0x00, 0x01, 0x21, 0x20];
    let cmd = LeRemoteConnectionParameterRequestNegativeReply::new(
        ConnHandle::new(0),
        RemoteConnectionParamsRejectReason::UnacceptableConnParameters,
    );
    assert_eq!(run(&cmd, &answer), Ok(()));
}

/// Execute `cmd` on an `ExternalController` whose controller answers each
/// write with `answer`, reading as trouble-host's RX runner does. The HCI
/// status of a failed command, or `Ok`.
fn run<C>(cmd: &C, answer: &[u8]) -> Result<(), u8>
where
    C: bt_hci::cmd::AsyncCmd,
    ExternalController<Scripted, 4>: ControllerCmdAsync<C>,
{
    let controller = ExternalController::<_, 4>::new(Scripted {
        answer: answer.to_vec(),
        queued: RefCell::new(VecDeque::new()),
    });
    let rx_loop = async {
        let mut rx = [0u8; 259];
        loop {
            let _ = bt_hci::controller::Controller::read(&controller, &mut rx).await;
        }
    };
    let outcome = block_on(select(controller.exec(cmd), rx_loop));
    match outcome {
        Either::First(Ok(())) => Ok(()),
        Either::First(Err(bt_hci::cmd::Error::Hci(e))) => Err(e.to_status().into_inner()),
        Either::First(Err(bt_hci::cmd::Error::Io(_))) => panic!("transport error"),
        Either::Second(never) => never,
    }
}

/// A controller that answers every command with one scripted packet.
struct Scripted {
    answer: Vec<u8>,
    queued: RefCell<VecDeque<Vec<u8>>>,
}

#[derive(Debug)]
struct ScriptedError;

impl core::fmt::Display for ScriptedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("scripted transport error")
    }
}

impl core::error::Error for ScriptedError {}

impl embedded_io_07::Error for ScriptedError {
    fn kind(&self) -> embedded_io_07::ErrorKind {
        embedded_io_07::ErrorKind::Other
    }
}

impl From<FromHciBytesError> for ScriptedError {
    fn from(_: FromHciBytesError) -> Self {
        Self
    }
}

impl embedded_io_07::ErrorType for Scripted {
    type Error = ScriptedError;
}

impl Transport for Scripted {
    async fn read<'a>(&self, rx: &'a mut [u8]) -> Result<ControllerToHostPacket<'a>, Self::Error> {
        let packet = poll_fn(|_| match self.queued.borrow_mut().pop_front() {
            Some(packet) => Poll::Ready(packet),
            // `block_on` polls again; the command's write queues the answer.
            None => Poll::Pending,
        })
        .await;
        rx[..packet.len()].copy_from_slice(&packet);
        Ok(ControllerToHostPacket::from_hci_bytes_complete(
            &rx[..packet.len()],
        )?)
    }

    async fn write<T: HostToControllerPacket>(&self, _val: &T) -> Result<(), Self::Error> {
        self.queued.borrow_mut().push_back(self.answer.clone());
        Ok(())
    }
}

/// A null-waker poll loop: every future here is ready within a few polls.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future);
    let waker = std::task::Waker::noop();
    let mut cx = core::task::Context::from_waker(waker);
    for _ in 0..1_000 {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
    }
    panic!("the command never completed");
}
