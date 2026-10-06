//! [`declare!`]: the one place seams are declared, and what it generates.

/// What a seam is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SeamKind {
    /// Hardware the emulator models faithfully but slowly. Off by default
    /// everywhere; on only for end-user simulation (Studio's Devices-page
    /// boards). Never in testing, never in `validate record`.
    Performance = 1,
    /// Hardware the emulator cannot model. On in every emulated run.
    Capability = 2,
}

/// How the firmware is shaped around a seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SeamShape {
    /// One real function; the emulator hooks its entry and answers.
    Replace = 1,
    /// An engaged check (an [`crate::engaged_byte!`]) at start-up plugs in a
    /// seam-backed adapter, whose calls are themselves hooked functions.
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

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Switch => "switch",
        }
    }
}

/// One declared seam, as both sides see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeamDecl {
    pub id: u16,
    /// The seam's short name (`ws281x_wait_step`).
    pub name: &'static str,
    /// The firmware's exported symbol: `lp_seam_<name>`.
    pub symbol: &'static str,
    pub kind: SeamKind,
    pub shape: SeamShape,
    pub doc: &'static str,
}

impl SeamDecl {
    pub fn by_id(id: u16) -> Option<&'static SeamDecl> {
        crate::ALL.iter().find(|d| d.id == id)
    }

    /// The immediate of the seam function's first instruction,
    /// `addi zero, zero, <hint>`: the id in the positive half of a 12-bit
    /// signed immediate. [`check`] proves it unique across the declarations,
    /// which is what keeps two seam bodies from ever being folded into one.
    pub const fn hint(&self) -> i32 {
        (self.id & 0x7ff) as i32
    }
}

/// The compile-time rules every declaration set must keep. Called from the
/// [`declare!`] expansion as a `const` item, so breaking one is a build
/// error, not a test failure:
///
/// - every id, and every [`SeamDecl::hint`], is unique;
/// - an id is in [`crate::test_seams::TEST_IDS`] exactly when the name starts
///   `test_`, and a test seam's doc starts [`crate::test_seams::DOC_PREFIX`];
/// - every symbol is `lp_seam_<name>`.
pub const fn check(all: &[SeamDecl]) {
    let mut i = 0;
    while i < all.len() {
        let a = &all[i];
        let test_named = starts_with(a.name, "test_");
        assert!(
            crate::test_seams::is_test_id(a.id) == test_named,
            "a seam id is in the test range exactly when its name starts `test_`"
        );
        if test_named {
            assert!(
                starts_with(a.doc, crate::test_seams::DOC_PREFIX),
                "a test seam's doc must start \"TEST ONLY: never in a shipped table\""
            );
        }
        assert!(starts_with(a.symbol, "lp_seam_"));
        let mut j = i + 1;
        while j < all.len() {
            assert!(a.id != all[j].id, "two seams share an id");
            assert!(a.hint() != all[j].hint(), "two seams share a body hint");
            j += 1;
        }
        i += 1;
    }
}

const fn starts_with(text: &str, prefix: &str) -> bool {
    let (t, p) = (text.as_bytes(), prefix.as_bytes());
    if t.len() < p.len() {
        return false;
    }
    let mut i = 0;
    while i < p.len() {
        if t[i] != p[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Declare every seam, once.
///
/// ```text
/// declare! {
///     seam 0x0001 ws281x_wait_step {
///         kind: Performance,
///         shape: Replace,
///         signature: fn() -> (),
///         doc: "…",
///     }
/// }
/// ```
///
/// Docs are `doc:` string literals, never `///`: a `///` comment reaches
/// `stringify!` as an attribute, and how one is rendered is exactly the kind
/// of non-whitespace difference two rustc versions can disagree on (see
/// [`crate::identity`]).
///
/// Generates, at the invocation site:
///
/// - `pub const DECLARATIONS: &str`, the invocation's own tokens;
/// - `pub const SEAM_ABI_ID: u64`, [`crate::identity::abi_id`] of them;
/// - `pub mod <name>` per seam, with `ID`, `NAME`, `SYMBOL`,
///   `ENGAGED_SYMBOL`, `KIND`, `SHAPE`, `HINT`, `DECL` and `Signature` (the
///   `extern "C"` function type the firmware's seam function must have —
///   [`crate::seam_fn!`] checks it);
/// - `pub const ALL: &[SeamDecl]`, and a `const` running [`check`] over it.
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
                /// The exported name of this seam's engaged byte, when it has
                /// one ([`crate::engaged_byte!`]).
                pub const ENGAGED_SYMBOL: &str = concat!("LP_SEAM_ENGAGED_", stringify!($name));
                pub const KIND: $crate::SeamKind = $crate::SeamKind::$kind;
                pub const SHAPE: $crate::SeamShape = $crate::SeamShape::$shape;
                pub const DECL: $crate::SeamDecl = $crate::SeamDecl {
                    id: $id,
                    name: stringify!($name),
                    symbol: concat!("lp_seam_", stringify!($name)),
                    kind: $crate::SeamKind::$kind,
                    shape: $crate::SeamShape::$shape,
                    doc: $doc,
                };
                /// The seam function's first instruction's immediate.
                pub const HINT: i32 = DECL.hint();
                /// The type the firmware's seam function must have.
                pub type Signature = extern "C" fn($($ty),*) -> $ret;
            }
        )*

        /// Every declared seam, in declaration order.
        pub const ALL: &[$crate::SeamDecl] = &[$($name::DECL),*];

        const _: () = $crate::__check_declarations(ALL);
    };
}
