use crate::ClientProjectView;
use lp_model::NodeHandle;

/// Assert first output channel RGB values
///
/// Output channels are RGB (3 bytes per channel). Checks that the first channel
/// has the expected red value
pub fn assert_first_output_red(
    client_view: &mut ClientProjectView,
    handle: NodeHandle,
    expected_r: u8,
) {
    let data = client_view.get_output_data(handle).unwrap();
    assert!(
        data.len() >= 3,
        "Output data should have at least 3 bytes (RGB) for first channel, got {}",
        data.len()
    );

    let r = data[0];
    let g = data[1];
    let b = data[2];

    assert_eq!(
        r, expected_r,
        "Output channel 0 R: expected {expected_r}, got {r}"
    );
    assert_eq!(g, 0, "Output channel 0 G: expected 0, got {g}");
    assert_eq!(b, 0, "Output channel 0 B: expected 0, got {b}");
}
