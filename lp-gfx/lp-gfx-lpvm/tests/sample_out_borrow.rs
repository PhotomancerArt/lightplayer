//! `LpGraphics::sample_out_data` borrows the CPU backend's sample-out in
//! place: what `write_sample_out` wrote is what the borrow reads, and a
//! clear shows through it — the fixture node's per-frame read is exactly
//! this borrow (no scratch copy).

use lp_gfx::LpGraphics;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_shader::ShaderFrontend;

#[test]
fn sample_out_data_borrows_what_write_sample_out_wrote() {
    let graphics = TargetLpvmGraphics::new(ShaderFrontend::LpsGlsl);
    let mut out = graphics.create_sample_out(2).expect("sample out");
    graphics
        .write_sample_out(&mut out, &[1, 2, 3, 4, 5, 6, 7, 8])
        .expect("write");

    assert_eq!(
        graphics.sample_out_data(&out).expect("borrow"),
        &[1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        graphics.sample_out_data(&out).expect("borrow"),
        graphics.read_sample_out(&out).expect("copy").as_slice(),
        "the borrow and the copying read see the same samples"
    );

    graphics.clear_sample_out(&mut out).expect("clear");
    assert_eq!(graphics.sample_out_data(&out).expect("borrow"), &[0; 8]);
}
