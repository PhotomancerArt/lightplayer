use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "wire",
    about = "Tools over the bytes a board writes on its link."
)]
pub struct WireCli {
    #[command(subcommand)]
    pub subcommand: WireSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum WireSubcommand {
    /// stdin → stdout: a capture of a board's link, rewritten as lines: each
    /// wire message as the `M!{json}` line it stands for (JSON or packed),
    /// each console line (log record or text outside frames) as itself.
    ///
    /// A board's USB link is an lp-link (frames, checksums, resends): the
    /// capture is read passively, resends are read once, and damaged frames
    /// (which the link resent) are counted, not written. Read a board's
    /// output (`?wire-capture=1`, a serial capture) as it is, or the host's
    /// side with `--from-host`:
    /// `lp-cli wire unpack < capture.bin | grep M!`.
    ///
    /// Packed replies are coded against a table both ends learn as the link
    /// runs, so a capture decodes them **from a link session's start** (the
    /// handshake). One that starts mid-session reads its messages unverified
    /// and cannot read the packed ones before the next session: each is
    /// written as `<unreadable message: …>` and counted, never guessed at.
    ///
    /// `--lines` reads an `M!`-line link instead (BLE, fw-emu): packed
    /// frames rewritten, every other byte passed through.
    Unpack(UnpackArgs),
}

#[derive(Debug, Args)]
pub struct UnpackArgs {
    /// Also print, on stderr, one line per message with its size on the
    /// wire and the size of its `M!{json}` line, then a total:
    ///
    ///   message <n> <json|packed> <payload_bytes> json <json_line_bytes>
    ///
    ///   total messages <n> packed <n> payload <bytes> json <bytes>
    ///     unreadable <n> damaged <n> gaps <n> sessions <n>
    ///
    /// `payload` counts the message's bytes on the link's proto channel (the
    /// link's own framing, checksums and acknowledgements are not attributed
    /// to messages); `json` counts `M!{json}\n`. With `--lines` the lines are
    /// the old `frame <n> packed <wire_bytes> json <json_line_bytes>` and
    /// `total frames …`.
    #[arg(long, verbatim_doc_comment)]
    pub sizes: bool,

    /// Read and write the `emu serve` wire-tap format
    /// (`<unix_us> <dir> <len>\n<bytes>\n`, `LP_EMU_WIRE_TAP`) instead of a
    /// raw byte stream: both directions (`<` board → host, `>` host → board)
    /// are read as the one link they are and written back as lines, and the
    /// tap's own annotations are dropped. The result reads like a tap of a
    /// link with no framing, for tools that know only `<` and `>`.
    #[arg(long, conflicts_with = "lines")]
    pub tap: bool,

    /// The capture is the host's side of the link (its requests), not the
    /// board's.
    #[arg(long, conflicts_with_all = ["tap", "lines"])]
    pub from_host: bool,

    /// The capture is of an `M!`-line link (BLE, fw-emu), not an lp-link
    /// (USB, or the classic's UART).
    #[arg(long)]
    pub lines: bool,
}
