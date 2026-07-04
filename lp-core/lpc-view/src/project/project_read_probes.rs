//! Probe-result extraction from a project-read event stream.
//!
//! The [`ProjectReadApplier`](super::ProjectReadApplier) deliberately does not
//! retain probe results on the [`ProjectView`](super::ProjectView): probes are
//! read-time diagnostics (render/control previews, slot explanations), not part
//! of the persistent project mirror. Consumers that still need probe results
//! (Studio's product previews, the lp-cli inspector) scan the event stream for
//! them.
//!
//! This is the single shared seam for that scan. The upcoming probe-chunking
//! phase (M6/P6) — which owns the wire probe event variants, the engine probe
//! arm, and the applier's probe handling — extends probe delivery here, so every
//! consumer picks up chunked probes without touching its own extraction code.

use alloc::vec::Vec;

use lpc_wire::{ProjectProbeResult, ProjectReadEvent, ProjectReadProbeEvent};

/// Collect every probe result carried by a project-read event stream, in stream
/// order.
///
/// A probe result self-identifies (M6/P3), so callers attribute each result by
/// its subject rather than by position. The returned slice preserves the order
/// the events arrived in.
#[must_use]
pub fn probe_results(events: &[ProjectReadEvent]) -> Vec<&ProjectProbeResult> {
    events
        .iter()
        .filter_map(|event| match event {
            ProjectReadEvent::Probe {
                event: ProjectReadProbeEvent::Result(result),
                ..
            } => Some(result),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpc_model::{NodeId, Revision, VisualProduct};
    use lpc_wire::RenderProductProbeResult;

    fn render_error(node: u32, message: &str) -> ProjectReadEvent {
        ProjectReadEvent::Probe {
            index: 0,
            event: ProjectReadProbeEvent::Result(ProjectProbeResult::RenderProduct(
                RenderProductProbeResult::Error {
                    product: VisualProduct::new(NodeId::new(node), 0),
                    message: String::from(message),
                },
            )),
        }
    }

    #[test]
    fn extracts_probe_results_in_order() {
        let events = vec![
            ProjectReadEvent::Begin {
                revision: Revision::new(1),
            },
            render_error(1, "a"),
            render_error(2, "b"),
            ProjectReadEvent::End {
                revision: Revision::new(1),
            },
        ];
        let probes = probe_results(&events);
        assert_eq!(probes.len(), 2);
        assert!(matches!(
            probes[0],
            ProjectProbeResult::RenderProduct(RenderProductProbeResult::Error { message, .. })
                if message == "a"
        ));
        assert!(matches!(
            probes[1],
            ProjectProbeResult::RenderProduct(RenderProductProbeResult::Error { message, .. })
                if message == "b"
        ));
    }

    #[test]
    fn no_probes_yields_empty() {
        let events = vec![
            ProjectReadEvent::Begin {
                revision: Revision::new(1),
            },
            ProjectReadEvent::End {
                revision: Revision::new(1),
            },
        ];
        assert!(probe_results(&events).is_empty());
    }
}
