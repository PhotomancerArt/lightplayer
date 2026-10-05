//! [`declare!`]: the one place seams are declared, and what it generates.

/// What a seam is for (plan, "Two kinds of seam").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SeamKind {
    /// Hardware the emulator models faithfully but slowly. Off by default
    /// everywhere; on only for end-user simulation.
    Performance = 1,
    /// Hardware the emulator cannot model. On in every emulated run.
    Capability = 2,
}

/// How the firmware is shaped around a seam (plan D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SeamShape {
    /// One real function; the emulator hooks its entry and answers.
    Replace = 1,
    /// An engaged check at start-up plugs in a seam-backed adapter, whose
    /// calls are themselves hooked functions.
    Switch = 2,
}

impl SeamKind {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Performance),
            2 => Some(Self::Capability),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Capability => "capability",
        }
    }
}

impl SeamShape {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Replace),
            2 => Some(Self::Switch),
            _ => None,
        }
    }
}

/// One declared seam, as both sides see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeamDecl {
    pub id: u16,
    /// The seam's short name (`ws281x_wait_step`).
    pub name: &'static str,
    /// The firmware's `#[no_mangle]` symbol: `lp_seam_<name>`.
    pub symbol: &'static str,
    pub kind: SeamKind,
    pub shape: SeamShape,
    pub doc: &'static str,
}

impl SeamDecl {
    pub fn by_id(id: u16) -> Option<&'static SeamDecl> {
        crate::ALL.iter().find(|d| d.id == id)
    }
}

/// Declare every seam, once.
///
/// Generates, at the invocation site:
///
/// - `pub const DECLARATIONS: &str`, the invocation's own tokens;
/// - `pub const SEAM_ABI_ID: u64`, [`crate::identity::abi_id`] of them;
/// - `pub mod <name>` per seam, with `ID`, `NAME`, `SYMBOL`, `KIND`, `SHAPE`
///   and `Signature` (the `extern "C"` function type the firmware's
///   `lp_seam_<name>` must have — `const _: Signature = lp_seam_x;` checks it);
/// - `pub const ALL: &[SeamDecl]`.
#[macro_export]
macro_rules! declare {
    ($($body:tt)*) => {
        /// The text [`SEAM_ABI_ID`] is computed over.
        pub const DECLARATIONS: &str = stringify!($($body)*);
        /// The seam identity: FNV-1a 64 of [`DECLARATIONS`] without whitespace.
        pub const SEAM_ABI_ID: u64 = $crate::identity::abi_id(DECLARATIONS);
        $crate::__seam_items!($($body)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __seam_items {
    ($(
        seam $id:literal $name:ident {
            kind: $kind:ident,
            shape: $shape:ident,
            signature: fn($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty,
            doc: $doc:literal $(,)?
        }
    )*) => {
        $(
            #[doc = $doc]
            pub mod $name {
                pub const ID: u16 = $id;
                pub const NAME: &str = stringify!($name);
                pub const SYMBOL: &str = concat!("lp_seam_", stringify!($name));
                pub const KIND: $crate::SeamKind = $crate::SeamKind::$kind;
                pub const SHAPE: $crate::SeamShape = $crate::SeamShape::$shape;
                /// The type the firmware's `lp_seam_*` function must have.
                pub type Signature = extern "C" fn($($ty),*) -> $ret;
            }
        )*

        /// Every declared seam, in declaration order.
        pub const ALL: &[$crate::SeamDecl] = &[$(
            $crate::SeamDecl {
                id: $id,
                name: stringify!($name),
                symbol: concat!("lp_seam_", stringify!($name)),
                kind: $crate::SeamKind::$kind,
                shape: $crate::SeamShape::$shape,
                doc: $doc,
            }
        ),*];
    };
}
