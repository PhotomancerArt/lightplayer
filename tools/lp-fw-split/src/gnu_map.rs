//! The input sections of a GNU ld map (`-Map`), as the section graph's
//! nodes — the S3's linker (`xtensa-esp32s3-elf-gcc`), whose map is not
//! lld's ([`crate::section_graph::parse_map`]).
//!
//! Read from the map's own shape (GNU ld's `-Map` documentation and the
//! files it writes): after the line `Linker script and memory map`, an
//! output section starts at column 0; an input section is one space then its
//! name, followed — on the same line, or on the next when the name is long —
//! by its address, its size and its object. Symbol lines (an address and a
//! name), `*fill*`, the input-section-description lines (`*(…)`) and
//! `(size before relaxing)` lines are not sections. Everything before that
//! line (the discarded sections, the memory configuration) is skipped.
//!
//! Nodes keep lld's description spelling, `object:(section)`, so the rest of
//! the pipeline reads both linkers' nodes the same way. The sizes are after
//! relaxation, like the addresses.

use crate::section_graph::Node;

/// The line that starts the part of the map that places sections.
const MAP_START: &str = "Linker script and memory map";

/// Every input section in a GNU ld map, with its output section's name as
/// [`normalise_out`] gives it.
pub fn parse_gnu_map(text: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut out = String::new();
    let mut pending: Option<String> = None;
    let mut started = false;
    for line in text.lines() {
        if !started {
            started = line.starts_with(MAP_START);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            let name = line.split_whitespace().next().unwrap_or("");
            out = normalise_out(name).to_string();
            pending = None;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let toks: Vec<&str> = line.split_whitespace().collect();
        if indent == 1 && !toks[0].starts_with('*') {
            let name = toks[0].to_string();
            match node_from(&out, &name, &toks[1..]) {
                Some(n) => {
                    nodes.push(n);
                    pending = None;
                }
                None => pending = Some(name),
            }
            continue;
        }
        if let Some(name) = pending.take()
            && let Some(n) = node_from(&out, &name, &toks)
        {
            nodes.push(n);
        }
    }
    nodes
}

/// The S3's output sections, by the name the shared graph gives their kind
/// ([`crate::section_graph::FLASH_OUTS`] and friends): `.vectors` is RAM code
/// the bootloader loads, `.rodata.wifi` flash rodata, the RTC slow sections
/// RAM data.
pub fn normalise_out(out: &str) -> &str {
    match out {
        ".vectors" => ".rwtext",
        ".rodata.wifi" => ".rodata",
        o if o.starts_with(".rtc_slow") => ".rtc_fast.data",
        o => o,
    }
}

/// `address size object…` after an input section's name.
fn node_from(out: &str, name: &str, toks: &[&str]) -> Option<Node> {
    if toks.len() < 3 {
        return None;
    }
    let hex = |t: &str| u64::from_str_radix(t.strip_prefix("0x")?, 16).ok();
    let vma = hex(toks[0])?;
    let size = hex(toks[1])?;
    Some(Node {
        vma,
        size,
        out: out.to_string(),
        desc: format!("{}:({name})", toks[2..].join(" ")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_sections_are_read_on_one_line_or_two() {
        let map = "\
Discarded input sections
 .text.gone     0x00000000        0x4 /x/a.o
Linker script and memory map

.text           0x42050020   0x1ff619
 *(.literal .text .literal.* .text.*)
 .literal.f     0x42050020       0x28 /x/a.rcgu.o
 .literal._RNvlong
                0x42050048        0x4 /x/a.rcgu.o
                                0x21c (size before relaxing)
 *fill*         0x4205004c        0x4
 .text.f        0x42050050       0x10 /x/lib.a(m.o)
                0x42050050                f
.vectors        0x40378000      0x400
 .vectors       0x40378000      0x400 /x/rt.o
";
        let nodes = parse_gnu_map(map);
        assert_eq!(nodes.len(), 4, "{nodes:?}");
        assert_eq!(nodes[0].desc, "/x/a.rcgu.o:(.literal.f)");
        assert_eq!((nodes[1].vma, nodes[1].size), (0x4205_0048, 4));
        assert_eq!(nodes[1].desc, "/x/a.rcgu.o:(.literal._RNvlong)");
        assert_eq!(nodes[2].object_and_section(), ("/x/lib.a(m.o)", ".text.f"));
        assert!(nodes[..3].iter().all(|n| n.out == ".text"));
        assert_eq!(nodes[3].out, ".rwtext", ".vectors is RAM code");
    }

    #[test]
    fn nothing_before_the_memory_map_is_a_node() {
        assert!(parse_gnu_map(" .text.x  0x42000000  0x4 /x/a.o\n").is_empty());
    }
}
