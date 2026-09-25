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
    /// stdin → stdout: rewrite every packed frame (JSON Pack,
    /// `\n 0x00 'P' COBS 0x00`) as the `M!{json}` line it stands for, and
    /// pass every other byte through untouched.
    ///
    /// Makes a capture readable to line tools:
    /// `lp-cli wire unpack < capture.bin | grep M!`. A frame that does not
    /// decode is written as nothing and reported on stderr.
    Unpack(UnpackArgs),
}

#[derive(Debug, Args)]
pub struct UnpackArgs {
    /// Also print, on stderr, one line per packed frame with its size on the
    /// wire and the size of the JSON line written in its place, then a total:
    ///
    ///   frame <n> packed <wire_bytes> json <json_line_bytes>
    ///
    ///   total frames <n> packed <bytes> json <bytes> errors <n>
    ///
    /// `packed` counts `0x00 'P' COBS 0x00`; `json` counts `M!{json}\n`.
    #[arg(long, verbatim_doc_comment)]
    pub sizes: bool,

    /// Read and write the `emu serve` wire-tap format
    /// (`<unix_us> <dir> <len>\n<bytes>\n`, `LP_EMU_WIRE_TAP`) instead of a
    /// raw byte stream: board → host records (`<`) are unpacked, host →
    /// board records (`>`) pass through, and the tap's own `P`/`E`
    /// annotations are dropped. The result reads like a tap of a link that
    /// never packed, for tools that know only `<` and `>`.
    #[arg(long)]
    pub tap: bool,
}
