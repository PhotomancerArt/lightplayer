//! Shared IP views: a register **layout** two chips genuinely share, with
//! every chip number supplied as a parameter.
//!
//! # The rule this module lives under
//!
//! The crate's standing rule is that it holds **no chip numbers**
//! (`lp-emu/esp/README.md`; [`crate::engine`]: *"a register offset, a bit
//! position, a reset value, an interrupt source number and a
//! [`RegGrade`](crate::periph::RegGrade) may not"* live in an engine). A
//! register **layout** is a different thing from a chip number, and this
//! module is the seam where that difference is written down:
//!
//! > An `ip` module may hold a register layout **only** when two chips' PACs
//! > agree on it offset-for-offset, verified and quoted. The base address,
//! > the aperture, the interrupt source number, the `regs` table and the
//! > grades are always the chip's, supplied as parameters. [`engine`] holds
//! > behaviour with no layout; `ip/` holds a layout two parts genuinely
//! > share. Anything that is neither is a chip's own view.
//!
//! [`engine`]: crate::engine
//!
//! So an `ip` view still owns its [`Peripheral`](crate::periph::Peripheral)
//! impl — which an engine never does — but it owns it against a `Config` the
//! chip crate writes out, and every number that could differ between two
//! parts is in that struct rather than in a `const` here.
//!
//! # What is here, and why it earned its place
//!
//! - [`usb_sj`] — USB-Serial-JTAG. The C6's and the S3's `usb_device` PACs
//!   agree offset-for-offset over the twenty registers either chip's drivers
//!   touch, including every bit the model lives on; the verification is
//!   quoted in that module's own docs. The S3 stops at `+0x048` where the C6
//!   has eight more registers, which is why the two registers the C6 view
//!   gives *behaviour* sit behind a capability the chip supplies rather than
//!   being unconditional.
//! - [`rmt`] — the remote-control transceiver. The S3's RMT **is** the C6's
//!   IP: `ch_tx_conf0`, `ch_rx_conf0/1`, `ch_tx_status`, `ch_rx_status`,
//!   `ch_tx_lim`, `ch_rx_lim`, `ch_rx_carrier_rm`, `sys_conf`, `tx_sim` and
//!   `ref_cnt_rst` exist on both by name with the same fields in the same
//!   order, and **none of them exists on the classic**, whose block keeps its
//!   own file (`m6/notes.md` §3.2, ruling D4). What differs is a table of
//!   offsets, five field positions and where the clock comes from — so the
//!   clock is a trait the chip implements and everything else is
//!   [`rmt::Config`]. The C6's committed transcripts are the gate on the
//!   move: the file's behaviour is unchanged, note for note.
//!
//! An IP view exists **only where the win is real**: the alternative to this
//! one was a 2,000-line copy of a file whose every behaviour is paid for by
//! a committed transcript, and a copy is where two models start to drift.

pub mod rmt;
pub mod usb_sj;
