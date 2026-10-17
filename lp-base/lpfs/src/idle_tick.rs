//! What a server tells its filesystem about each tick (`LpFs::idle_tick`),
//! so a filesystem with cleanup to do (the tree store) can do it in the
//! time a tick leaves over. No clock is read here: the time is the server's
//! own injected `delta_ms` (sans-IO).

/// One server tick, as the filesystem's idle work sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdleTick {
    /// The time since the last tick: the last frame, whole (its render, its
    /// requests and whatever idle work it ran).
    pub delta_ms: u32,
    /// This tick handled a request that changes files, or a batch is open.
    pub busy: bool,
    /// A show is playing: every tick renders a frame someone is watching.
    pub playing: bool,
}
