//! One tab holds a board: the hold vocabulary, the book, the edge and its
//! host double, and the pure halves of the holder's flows.
//!
//! A USB board's port opens in one tab only (the OS refuses a second
//! `open()`), and a board's network slot takes one client. So that another
//! Studio tab of the same browser can say "Open in another tab" rather than
//! "in use by another app", ask the holder to let go, and notice it died,
//! the tab that holds a board takes a Web Lock named for it after the
//! board's hello ([`HoldKey`]) and says so on a channel ([`HoldNote`]).
//! [`BoardHoldBook`] is what this tab knows; [`BoardHoldEdge`] is the one
//! door to the browser; [`MemoryBoardHoldBus`] is that door on the host.
//!
//! The flows the controller runs on them, each with its pure half here:
//! [`HoldPriming`] (learn what is held before the first sweep),
//! [`hold_reconcile`] (claim, release, level, the facts on the boards),
//! [`UsbHoldGate`] (the ports never opened, and which board a held port
//! is), [`hold_answer`] (the holder's side of an ask) and
//! [`BoardHoldFlow`] (what the controller keeps between them). The connect
//! that takes a board over is an offer built in core, at
//! `devices/<board ref>/take-over` (`take_over_offer`).

pub mod board_hold_edge;
pub mod hold_answer;
pub mod hold_book;
pub mod hold_edge_event;
pub mod hold_flow;
pub mod hold_gate;
pub mod hold_key;
pub mod hold_note;
pub mod hold_priming;
pub mod hold_reconcile;
pub mod memory_board_hold;
pub mod tab_id;

pub use board_hold_edge::{BoardHoldEdge, ClaimAnswer};
pub use hold_answer::{
    AnswerPlan, PendingRelease, RELEASE_CLOSE_PATIENCE_SECS, ReleaseStage, answer_plan,
};
pub use hold_book::{BoardHoldBook, BookChange, OtherHold, PendingAsk};
pub use hold_edge_event::HoldEdgeEvent;
pub use hold_flow::BoardHoldFlow;
pub use hold_gate::{
    SharedUsbHoldGate, UsbHoldGate, associate, gate_group, reads_as_held, usb_pair_of,
};
pub use hold_key::{HoldKey, LOCK_PREFIX, UsbPair, is_network_road};
pub use hold_note::{AskOutcome, AskRefusal, HOLD_PROTO_VERSION, HoldNote};
pub use hold_priming::{HoldPriming, PRIMING_PATIENCE_SECS};
pub use hold_reconcile::{
    HoldCandidate, HoldPlan, desired_facts, desired_holds, fact_changes, hold_level, plan_holds,
};
pub use memory_board_hold::{MemoryBoardHold, MemoryBoardHoldBus};
pub use tab_id::TabId;
