//! Parse a linked Xtensa ELF32 and load its `PT_LOAD` segments into
//! [`lp_xt_emu`] memory.

use lp_xt_emu::Emulator;
use object::{Architecture, Object, ObjectSection, ObjectSegment, ObjectSymbol};

/// Why an ELF could not be parsed or loaded.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ElfError {
    /// The `object` crate rejected the file outright.
    Parse(String),
    /// Parsed, but not a little-endian 32-bit Xtensa image.
    NotXtensaElf32 {
        architecture: String,
        is_64: bool,
        is_little_endian: bool,
    },
    /// Not a linked executable (e.g. a relocatable `.o` — see the `reloc`
    /// feature's linker driver for those).
    NotExecutable { kind: String },
    /// The file carries REL/RELA relocation sections. Linked executables are
    /// pre-resolved; relocation processing lives behind the `reloc` feature.
    HasRelocations { section: String },
    /// A `PT_LOAD` segment's contents could not be read from the file.
    SegmentData { vaddr: u32 },
    /// A `PT_LOAD` segment does not fit the emulator's modeled memory.
    Unmapped { vaddr: u32, memsz: u32 },
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ElfError::Parse(e) => write!(f, "ELF parse error: {e}"),
            ElfError::NotXtensaElf32 {
                architecture,
                is_64,
                is_little_endian,
            } => write!(
                f,
                "not a little-endian Xtensa ELF32 (arch={architecture}, 64-bit={is_64}, \
                 little-endian={is_little_endian})"
            ),
            ElfError::NotExecutable { kind } => {
                write!(
                    f,
                    "not a linked executable (object kind: {kind}); relocatable objects need the \
                     `reloc` feature's linker driver"
                )
            }
            ElfError::HasRelocations { section } => write!(
                f,
                "ELF has relocations (section {section}); linked-executable loading only — \
                 relocation processing lives behind the `reloc` feature"
            ),
            ElfError::SegmentData { vaddr } => {
                write!(
                    f,
                    "could not read segment data for PT_LOAD at {vaddr:#010x}"
                )
            }
            ElfError::Unmapped { vaddr, memsz } => write!(
                f,
                "PT_LOAD segment at {vaddr:#010x} ({memsz:#x} bytes) does not fit the emulator's \
                 modeled memory (see lp-xt-emu memory.rs and the fixture linker script)"
            ),
        }
    }
}

impl std::error::Error for ElfError {}

/// One `PT_LOAD` segment, decoded to the pieces the emulator needs.
#[derive(Clone, Copy, Debug)]
pub struct Segment<'d> {
    /// Load address (`p_vaddr`).
    pub vaddr: u32,
    /// File-backed bytes (`p_filesz` long).
    pub data: &'d [u8],
    /// Total in-memory size (`p_memsz`); the tail beyond `data` is zeroed.
    pub memsz: u32,
}

/// A parsed, validated, linked Xtensa ELF32 executable.
pub struct XtensaElf<'d> {
    file: object::File<'d>,
}

impl core::fmt::Debug for XtensaElf<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("XtensaElf")
            .field("entry", &self.entry())
            .finish_non_exhaustive()
    }
}

impl<'d> XtensaElf<'d> {
    /// Parse and validate: ELF32, little-endian, `e_machine == EM_XTENSA` (94),
    /// a linked executable, and free of REL/RELA sections.
    pub fn parse(data: &'d [u8]) -> Result<XtensaElf<'d>, ElfError> {
        let file = object::File::parse(data).map_err(|e| ElfError::Parse(e.to_string()))?;

        if file.architecture() != Architecture::Xtensa || file.is_64() || !file.is_little_endian() {
            return Err(ElfError::NotXtensaElf32 {
                architecture: format!("{:?}", file.architecture()),
                is_64: file.is_64(),
                is_little_endian: file.is_little_endian(),
            });
        }
        if file.kind() != object::ObjectKind::Executable {
            return Err(ElfError::NotExecutable {
                kind: format!("{:?}", file.kind()),
            });
        }
        for section in file.sections() {
            if section.relocations().next().is_some() {
                return Err(ElfError::HasRelocations {
                    section: section.name().unwrap_or("<unnamed>").to_string(),
                });
            }
        }
        Ok(XtensaElf { file })
    }

    /// The ELF entry point (`e_entry`) — an I-bus address for our fixtures.
    pub fn entry(&self) -> u32 {
        self.file.entry() as u32
    }

    /// Address of a named symbol (e.g. `_start` or a test function).
    pub fn symbol(&self, name: &str) -> Option<u32> {
        self.file
            .symbols()
            .find(|s| s.name() == Ok(name))
            .map(|s| s.address() as u32)
    }

    /// Every named symbol as `(name, address)`.
    ///
    /// [`symbol`](Self::symbol) answers one lookup; a host engine linking
    /// compiled code against this image needs the whole map so it can resolve
    /// call relocations by name. Unnamed symbols are skipped.
    pub fn symbols(&self) -> Vec<(String, u32)> {
        self.file
            .symbols()
            .filter_map(|s| match s.name() {
                Ok(n) if !n.is_empty() => Some((n.to_string(), s.address() as u32)),
                _ => None,
            })
            .collect()
    }

    /// The `PT_LOAD` segments (the `object` segment iterator yields only
    /// loadable segments).
    pub fn segments(&self) -> Result<Vec<Segment<'d>>, ElfError> {
        let mut out = Vec::new();
        for seg in self.file.segments() {
            let vaddr = seg.address() as u32;
            let data = seg.data().map_err(|_| ElfError::SegmentData { vaddr })?;
            out.push(Segment {
                vaddr,
                data,
                memsz: seg.size() as u32,
            });
        }
        Ok(out)
    }

    /// Copy every `PT_LOAD` segment into the emulator's memory at its
    /// `p_vaddr`, zero-filling the `p_memsz` tail (`.bss`). Fails without
    /// side-channel panics if a segment falls outside the modeled regions.
    ///
    /// Goes through `Memory`'s **loader** path, not guest stores: an image's
    /// `.text`/`.rodata` land in the read-only flash windows, where a guest
    /// store faults by design. Placing an image there is what a flasher does.
    pub fn load_into(&self, emu: &mut Emulator) -> Result<(), ElfError> {
        for seg in self.segments()? {
            let unmapped = |_| ElfError::Unmapped {
                vaddr: seg.vaddr,
                memsz: seg.memsz,
            };
            emu.mem
                .try_load_bytes(seg.vaddr, seg.data)
                .map_err(unmapped)?;
            let tail = seg.memsz.saturating_sub(seg.data.len() as u32);
            emu.mem
                .try_zero(seg.vaddr.wrapping_add(seg.data.len() as u32), tail)
                .map_err(unmapped)?;
        }
        Ok(())
    }
}
