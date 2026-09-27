//! A deterministic, seeded simulator that runs two [`Link`](crate::Link)s
//! against each other over fault-injecting pipes, checks the delivery
//! property as messages arrive, and measures goodput, latency, overhead and
//! memory. Feature `sim` (host only).

pub mod checker;
pub mod endpoint;
pub mod pipe;
pub mod probe_message;
pub mod scenario;
pub mod sim_rng;
pub mod transport;
pub mod workload;

pub use pipe::{Faults, PipeModel};
pub use scenario::{Report, Scenario, run};
pub use transport::Transport;
pub use workload::Workload;
