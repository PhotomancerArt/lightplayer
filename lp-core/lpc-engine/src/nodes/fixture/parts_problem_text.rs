//! A format-6 fixture's problems in plain words, for its status (lean Q42):
//! what the patcher reported, and what this build refused.
//!
//! An override whose node is gone ([`Problem::GoneNode`]) says nothing here:
//! the patcher keeps it and nothing goes dark because of it; Studio shows it
//! (M4).

use alloc::format;
use alloc::string::String;

use lpc_mapping::patching::{PatchTarget, Patching, Problem};
use lpc_model::ChannelName;

use crate::nodes::fixture::patching_runs::Refusal;

/// `problem` in words, or `None` when it darkens nothing.
pub(crate) fn problem_text(
    problem: &Problem,
    patching: &Patching,
    channel: Option<&ChannelName>,
) -> Option<String> {
    Some(match problem {
        Problem::NoStart => String::from(
            "the fixture has no start: nothing says which output and port its lamps begin on",
        ),
        Problem::GoneNode(_) => return None,
        Problem::GoneOutput { output, set_by } => match channel {
            Some(channel) => format!(
                "{} names output \"{output}\", but no named output reads bus:{}",
                setter(set_by),
                channel.0
            ),
            None => format!(
                "{} names output \"{output}\", but this fixture publishes on no bus channel",
                setter(set_by)
            ),
        },
        Problem::GonePort {
            output,
            port,
            set_by,
        } => format!(
            "{} names port {port} of output \"{output}\", which has no port {port}",
            setter(set_by)
        ),
        Problem::DoesntFit { segment, port } => {
            let output = patching
                .segment(segment)
                .map(|laid| laid.address.output.as_str())
                .unwrap_or("?");
            format!(
                "{segment} doesn't fit what is left of port {port} of output \"{output}\" \
                 where it was set; only the lamps the port reaches light"
            )
        }
        Problem::PastLastPort { segment } => format!(
            "{segment} runs past the last port; it and what follows it stay dark until a \
             start sets a port"
        ),
        Problem::OffLampBoundary { segment } => {
            let channel = patching
                .segment(segment)
                .map_or(0, |laid| laid.address.channel);
            off_lamp_boundary(&format!("{segment}"), channel)
        }
        Problem::Overlap { a, b, output, port } => {
            format!("{a} and {b} share channels on port {port} of output \"{output}\"")
        }
        Problem::OrderMismatch { segment } => {
            format!("{segment}'s order doesn't fit its colors")
        }
    })
}

/// A refused segment in words.
pub(crate) fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::OffLampBoundary { segment, channel } => {
            off_lamp_boundary(&format!("{segment}"), *channel)
        }
        Refusal::Colors { segment, colors } => format!(
            "{segment} is {}; this build drives RGB lamps only",
            colors_word(*colors)
        ),
        Refusal::Order {
            segment,
            order,
            fixture,
        } => format!(
            "{segment}'s order is {order} but the fixture's is {}; per-segment order comes \
             with the lamp model",
            fixture.as_str().to_ascii_uppercase()
        ),
    }
}

fn off_lamp_boundary(segment: &str, channel: u32) -> String {
    format!(
        "{segment} starts part-way into a lamp, at channel {channel}; this build places lamps \
         on whole-lamp boundaries"
    )
}

fn setter(set_by: &PatchTarget) -> String {
    match set_by {
        PatchTarget::Fixture => String::from("the fixture's start"),
        PatchTarget::Part(part) => format!("{part}'s start"),
        PatchTarget::Node(node) => format!("the override on {node}"),
    }
}

fn colors_word(colors: lpc_mapping::part::Colors) -> &'static str {
    use lpc_mapping::part::Colors;
    match colors {
        Colors::Rgb => "RGB",
        Colors::Rgbw => "RGBW",
        Colors::Cct => "CCT",
        Colors::Mono => "mono",
        Colors::RgbCct => "RGB+CCT",
    }
}
