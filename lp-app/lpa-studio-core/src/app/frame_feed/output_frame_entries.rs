//! Reassembling a card-feed read's event stream into published-frame entries.

/// The published-frame entries carried by a card-feed read's event stream.
///
/// The feed asks for exactly one probe, so this walks the stream for probe
/// index 0 and reassembles it: a small result arrives whole, and a
/// dome-scale frame arrives as a header plus bounded chunks the transport
/// already validated for coverage. Anything else in the stream (the
/// begin/end revision markers) is not this read's business.
///
/// A malformed stream yields no entries rather than an error: the feed's
/// answer to "no frame this time" is to keep the last one, and there is no
/// user-facing failure to raise for a picture that did not arrive.
pub fn output_frame_entries(
    events: &[lpc_wire::ProjectReadEvent],
) -> Vec<lpc_wire::OutputFrameEntry> {
    use lpc_wire::{
        OutputFrameProbeResult, ProjectProbeResult, ProjectProbeResultHeader, ProjectReadEvent,
        ProjectReadProbeEvent,
    };

    let mut pending: Option<(ProjectProbeResultHeader, Vec<u8>)> = None;
    for event in events {
        let ProjectReadEvent::Probe { event, .. } = event else {
            continue;
        };
        match event {
            ProjectReadProbeEvent::Result(ProjectProbeResult::OutputFrame(
                OutputFrameProbeResult::Frame { outputs },
            )) => return outputs.clone(),
            ProjectReadProbeEvent::ResultBegin { header, .. } => {
                pending = Some((header.clone(), Vec::new()));
            }
            ProjectReadProbeEvent::ResultBytes { bytes, .. } => {
                if let Some((_, buffer)) = pending.as_mut() {
                    buffer.extend_from_slice(bytes);
                }
            }
            ProjectReadProbeEvent::ResultEnd => {
                let Some((header, bytes)) = pending.take() else {
                    continue;
                };
                if let ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs }) =
                    header.into_result(bytes)
                {
                    return outputs;
                }
            }
            _ => {}
        }
    }
    Vec::new()
}
