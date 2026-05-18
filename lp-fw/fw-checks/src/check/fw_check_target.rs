use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwCheckTarget {
    Esp32C6,
    FwEmu,
}

impl FwCheckTarget {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Esp32C6 => "esp32c6",
            Self::FwEmu => "fw-emu",
        }
    }
}

impl fmt::Display for FwCheckTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}
