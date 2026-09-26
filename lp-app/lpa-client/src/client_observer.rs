//! An injected observer of every request a client sends and what became of
//! it — the session recorder's view of the protocol (Studio's
//! `?record=<url>`).
//!
//! The client stays runtime-neutral: it reads no clock and knows no
//! platform. It reports *what* happened, in order, and the edge that
//! installed the observer stamps *when* (and so computes each request's
//! latency). With no observer installed, each report is one thread-local
//! check; the observation itself is only built when someone is listening.
//!
//! Installed per thread rather than per [`LpClient`](crate::LpClient):
//! Studio builds a client for every conversation (the editor lens, a push,
//! a manifest stamp, an open), and a recorder that had to be threaded into
//! each construction would miss the one nobody remembered. Every
//! [`LpClient`](crate::LpClient) request and every
//! [`run_project_read`](crate::run_project_read) on the thread reports.
//!
//! What is observed:
//!
//! - [`ClientObservation::Sent`] once a request is on the wire;
//! - [`ClientObservation::Frame`] for each frame that arrived while the
//!   request waited — its own (a single reply, or each frame of a streamed
//!   project read, with `seq`/`fin`) and the ones it set aside (a stale
//!   reply, a previous owner's, an uncorrelated id, a server-originated
//!   frame under a request id). Frames on the unsolicited id 0 (heartbeats,
//!   logs) are not reported: they are traffic, not answers, and the byte
//!   tap already carries them;
//! - [`ClientObservation::Outcome`] once: answered, failed (a transport
//!   error — including a `ClientIo`'s own "did not respond within 5.0s" —
//!   a server error, or a stream protocol error such as "expected project
//!   read frame seq 0, got 1"), timed out on the client's own deadline, or
//!   cancelled.

use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpc_wire::{ClientRequest, FsRequest, WireServerMessage};

use crate::protocol_session::ResponseDisposition;

/// One thing a client did or saw, for request `id` of client
/// `conversation` ([`ProtocolSession::conversation`](crate::protocol_session::ProtocolSession::conversation)).
#[derive(Clone, Debug, PartialEq)]
pub enum ClientObservation {
    /// The request went out. `kind` names the request, never its payload.
    Sent {
        conversation: u64,
        id: u64,
        kind: &'static str,
    },
    /// A frame arrived while request `id` waited.
    Frame {
        conversation: u64,
        id: u64,
        /// The id the frame carried.
        response_id: u64,
        seq: u32,
        fin: bool,
        disposition: FrameDisposition,
    },
    /// What became of request `id`. Reported once.
    Outcome {
        conversation: u64,
        id: u64,
        outcome: RequestOutcome,
    },
}

impl ClientObservation {
    /// The client conversation it belongs to: request ids are per client
    /// (two clients each send a request `1`), so `(conversation, id)` is
    /// what names one request.
    pub fn conversation(&self) -> u64 {
        match self {
            Self::Sent { conversation, .. }
            | Self::Frame { conversation, .. }
            | Self::Outcome { conversation, .. } => *conversation,
        }
    }

    /// The request id it is about.
    pub fn id(&self) -> u64 {
        match self {
            Self::Sent { id, .. } | Self::Frame { id, .. } | Self::Outcome { id, .. } => *id,
        }
    }
}

/// How a frame that arrived during a request was classified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameDisposition {
    /// The request's own.
    Matched,
    /// A server-originated frame (a hello, a log) under a request id.
    ServerOriginated,
    /// A late reply to a request this client abandoned.
    StaleAbandoned,
    /// A reply to the wire's previous owner.
    PriorOwner,
    /// An id this client neither waits for nor abandoned.
    Uncorrelated,
}

impl FrameDisposition {
    /// The recorder's rendering.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::ServerOriginated => "server-originated",
            Self::StaleAbandoned => "stale",
            Self::PriorOwner => "prior-owner",
            Self::Uncorrelated => "uncorrelated",
        }
    }
}

/// How a request ended.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestOutcome {
    /// Its answer arrived (the final frame, for a stream).
    Answered,
    /// It failed: the error's message.
    Failed { error: String },
    /// The client's own deadline ran out: the total budget of a single
    /// request, or the quiet gap between a stream's frames.
    TimedOut { budget: Duration },
    /// The caller walked away (a cancelled pull).
    Cancelled,
}

impl RequestOutcome {
    /// The recorder's rendering.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::Failed { .. } => "failed",
            Self::TimedOut { .. } => "timed-out",
            Self::Cancelled => "cancelled",
        }
    }
}

/// The callback an edge installs.
pub type ClientObserver = Rc<dyn Fn(&ClientObservation)>;

thread_local! {
    static OBSERVER: RefCell<Option<ClientObserver>> = const { RefCell::new(None) };
}

/// Install (`Some`) or remove (`None`) this thread's observer.
pub fn set_client_observer(observer: Option<ClientObserver>) {
    OBSERVER.with(|slot| *slot.borrow_mut() = observer);
}

/// Report one observation, built only when an observer is installed.
pub(crate) fn observe(build: impl FnOnce() -> ClientObservation) {
    let observer = OBSERVER.with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()));
    if let Some(observer) = observer {
        observer(&build());
    }
}

/// Report a frame that arrived while request `request_id` waited. Frames on
/// the unsolicited id (heartbeats, logs) are traffic, not answers, and are
/// not reported.
pub(crate) fn observe_frame(
    conversation: u64,
    request_id: u64,
    response: &WireServerMessage,
    disposition: &ResponseDisposition,
) {
    let disposition = match disposition {
        ResponseDisposition::Matched => FrameDisposition::Matched,
        ResponseDisposition::Unsolicited => return,
        ResponseDisposition::ServerOriginated { .. } => FrameDisposition::ServerOriginated,
        ResponseDisposition::StaleAbandoned { .. } => FrameDisposition::StaleAbandoned,
        ResponseDisposition::PriorOwner { .. } => FrameDisposition::PriorOwner,
        ResponseDisposition::Uncorrelated { .. } => FrameDisposition::Uncorrelated,
    };
    observe(|| ClientObservation::Frame {
        conversation,
        id: request_id,
        response_id: response.id,
        seq: response.seq,
        fin: response.fin,
        disposition,
    });
}

/// The name a request is recorded under: its variant, and the operation
/// for a filesystem request (`fs.read`). Exhaustive, so a new request
/// variant has to be named here.
pub fn request_kind(request: &ClientRequest) -> &'static str {
    match request {
        ClientRequest::Hello => "hello",
        ClientRequest::Filesystem(fs) => match fs {
            FsRequest::Read { .. } => "fs.read",
            FsRequest::Write { .. } => "fs.write",
            FsRequest::DeleteFile { .. } => "fs.delete-file",
            FsRequest::DeleteDir { .. } => "fs.delete-dir",
            FsRequest::ListDir { .. } => "fs.list-dir",
            FsRequest::ChangesSince { .. } => "fs.changes-since",
            FsRequest::WriteChunk { .. } => "fs.write-chunk",
            FsRequest::HashPackage { .. } => "fs.hash-package",
        },
        ClientRequest::LoadProject { .. } => "project.load",
        ClientRequest::UnloadProject { .. } => "project.unload",
        ClientRequest::ProjectRead { .. } => "project.read",
        ClientRequest::ProjectCommand { .. } => "project.command",
        ClientRequest::ListAvailableProjects => "project.list-available",
        ClientRequest::ListLoadedProjects => "project.list-loaded",
        ClientRequest::StopAllProjects => "project.stop-all",
        ClientRequest::SetLogLevel { .. } => "set-log-level",
        ClientRequest::Reboot => "reboot",
        ClientRequest::ClearFaults => "clear-faults",
        ClientRequest::SetEncoding { .. } => "set-encoding",
        ClientRequest::LoginBegin => "login.begin",
        ClientRequest::LoginAnswer { .. } => "login.answer",
        ClientRequest::AccessList => "access.list",
        ClientRequest::AccessAdd { .. } => "access.add",
        ClientRequest::AccessRemove { .. } => "access.remove",
        ClientRequest::AccessSetSwitches { .. } => "access.set-switches",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_observer_nothing_is_built() {
        set_client_observer(None);
        observe(|| panic!("an observation was built with nobody listening"));
    }

    #[test]
    fn an_installed_observer_hears_each_observation() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&seen);
        set_client_observer(Some(Rc::new(move |observation: &ClientObservation| {
            sink.borrow_mut().push(observation.clone());
        })));
        let sent = |id| ClientObservation::Sent {
            conversation: 1,
            id,
            kind: "hello",
        };
        observe(|| sent(4));
        set_client_observer(None);
        observe(|| sent(5));
        assert_eq!(*seen.borrow(), vec![sent(4)]);
    }

    #[test]
    fn a_request_is_named_without_its_payload() {
        assert_eq!(request_kind(&ClientRequest::Hello), "hello");
        assert_eq!(
            request_kind(&ClientRequest::LoadProject {
                path: "/projects/x".to_string()
            }),
            "project.load"
        );
    }
}
