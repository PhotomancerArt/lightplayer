//! Host-side perf event types (mirrors guest `lp-perf` naming).

pub const MAX_EVENT_NAME_LEN: usize = 64;

pub const EVENT_FRAME: &str = "frame";

/// Host-only synthetic marker: session began (see [`crate::profile::ProfileSession::start`]).
pub const EVENT_PROFILE_START: &str = "profile:start";
/// Host-only synthetic marker: session ended (see [`crate::profile::ProfileSession::end`]).
pub const EVENT_PROFILE_END: &str = "profile:end";

/// ⚠️ A guest marker whose name is not in this list is dropped by the
/// emulator run loop (`intern_known_name` returns `None`), silently. Every
/// `lp_perf::EVENT_*` constant must have its string here.
pub static KNOWN_EVENT_NAMES: &[&str] = &[
    "frame",
    "shader-compile",
    "shader-link",
    "project-load",
    "project-read",
    "server-boot",
    "profile:start",
    "profile:end",
];

/// Linear scan over [`KNOWN_EVENT_NAMES`]; returns the static slice on hit.
pub fn intern_known_name(s: &str) -> Option<&'static str> {
    for &name in KNOWN_EVENT_NAMES {
        if name == s {
            return Some(name);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct PerfEvent {
    pub cycle: u64,
    pub name: &'static str,
    pub kind: PerfEventKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PerfEventKind {
    Begin = 0,
    End = 1,
    Instant = 2,
}

impl PerfEventKind {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Begin),
            1 => Some(Self::End),
            2 => Some(Self::Instant),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Begin => "B",
            Self::End => "E",
            Self::Instant => "I",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EVENT_FRAME, PerfEventKind, intern_known_name};

    #[test]
    fn perf_event_kind_from_u32_round_trips() {
        for (raw, expected) in [
            (0u32, PerfEventKind::Begin),
            (1u32, PerfEventKind::End),
            (2u32, PerfEventKind::Instant),
        ] {
            let k = PerfEventKind::from_u32(raw).expect("valid kind");
            assert_eq!(k, expected);
            assert_eq!(k as u32, raw);
        }
        assert!(PerfEventKind::from_u32(3).is_none());
    }

    #[test]
    fn intern_known_name_frame_and_unknown() {
        let s = intern_known_name("frame").expect("frame is known");
        assert_eq!(s, EVENT_FRAME);
        assert_eq!(s, "frame");
        assert!(intern_known_name("xyz").is_none());
    }

    /// ⚠️ The one that bites: a marker name the guest emits but this list
    /// does not carry is dropped silently, so the window simply never appears
    /// in the trace and the analysis quietly measures a shorter run.
    #[test]
    fn every_window_the_server_opens_is_interned() {
        for name in [
            "frame",
            "shader-compile",
            "shader-link",
            "project-load",
            "project-read",
            "server-boot",
        ] {
            assert_eq!(
                intern_known_name(name),
                Some(name),
                "{name} must be interned or its window is dropped silently"
            );
        }
    }

    #[test]
    fn intern_known_name_profile_markers_round_trip() {
        use super::{EVENT_PROFILE_END, EVENT_PROFILE_START};

        let s = intern_known_name("profile:start").expect("profile:start is known");
        assert_eq!(s, EVENT_PROFILE_START);
        let e = intern_known_name("profile:end").expect("profile:end is known");
        assert_eq!(e, EVENT_PROFILE_END);
    }
}
