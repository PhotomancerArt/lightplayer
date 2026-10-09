//! One tab holds a board: the hold vocabulary, the book, the edge and its
//! host double.
//!
//! A USB board's port opens in one tab only (the OS refuses a second
//! `open()`), and a board's network slot takes one client. So that another
//! Studio tab of the same browser can say "Open in another tab" rather than
//! "in use by another app", ask the holder to let go, and notice it died,
//! the tab that holds a board takes a Web Lock named for it after the
//! board's hello ([`HoldKey`]) and says so on a channel ([`HoldNote`]).
//! [`BoardHoldBook`] is what this tab knows; [`BoardHoldEdge`] is the one
//! door to the browser; [`MemoryBoardHoldBus`] is that door on the host.
//! The connect that takes a board over is an offer built in core, at
//! `devices/<board ref>/take-over` (still to come).

pub mod board_hold_edge;
pub mod hold_book;
pub mod hold_key;
pub mod hold_note;
pub mod memory_board_hold;
pub mod tab_id;

pub use board_hold_edge::{BoardHoldEdge, ClaimAnswer};
pub use hold_book::{BoardHoldBook, BookChange, OtherHold, PendingAsk};
pub use hold_key::{HoldKey, LOCK_PREFIX, UsbPair};
pub use hold_note::{AskOutcome, AskRefusal, HOLD_PROTO_VERSION, HoldNote};
pub use memory_board_hold::{MemoryBoardHold, MemoryBoardHoldBus};
pub use tab_id::TabId;
