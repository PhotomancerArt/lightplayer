//! SPIKE: dump every variant's dye field, step by step, for the look check.
//! `cargo run -p lpc-engine --release --example fluid_look -- <out-dir> [steps]`
//! writes `<out>/<grid>/<variant>.f32`: per step, r, g, b over the interior
//! (n×n each), little-endian f32. `spikes/fluid/look.py` reads them.

use std::io::Write;

use lpc_engine::nodes::fluid::spike::{Num, Opts, SpikeSolver, Sweep, scripted_step};
use lps_q32::Q32;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::path::PathBuf::from(&args[1]);
    let steps: u32 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(750);
    let rb = |w: f32| Sweep::RedBlack { omega: w };
    let e = Opts::EXACT;
    // (name, number, opts)
    let v = |vi: usize, p: usize, sw: Sweep, warm: bool, src: bool| Opts {
        vel_iters: Some(vi),
        p_iters: Some(p),
        warm_p: warm,
        sweep: sw,
        vel_from_src: src,
        ..e
    };
    let gs = Sweep::Gs;
    let rows: Vec<(&str, char, Opts)> = vec![
        ("base", 'q', Opts::BASE),
        ("exact-qw", 'w', e),
        ("f32", 'f', e),
        ("vel2", 'q', Opts { vel_iters: Some(2), ..e }),
        ("vel1", 'q', Opts { vel_iters: Some(1), ..e }),
        ("vel1-src", 'q', Opts { vel_iters: Some(1), vel_from_src: true, ..e }),
        ("v1s-p7-gs", 'q', v(1, 7, gs, false, true)),
        ("v1s-p5-gs", 'q', v(1, 5, gs, false, true)),
        ("v1s-p4-gs", 'q', v(1, 4, gs, false, true)),
        ("v1s-p3-gs", 'q', v(1, 3, gs, false, true)),
        ("v1s-p5-gs-warm", 'q', v(1, 5, gs, true, true)),
        ("v1s-p10-rb", 'q', v(1, 10, rb(1.0), false, true)),
        ("v1s-p5-rb", 'q', v(1, 5, rb(1.0), false, true)),
        ("v1s-p5-rb1.2", 'q', v(1, 5, rb(1.2), false, true)),
        ("v1s-p4-rb1.3", 'q', v(1, 4, rb(1.3), false, true)),
        ("v1s-p3-rb1.5", 'q', v(1, 3, rb(1.5), false, true)),
        ("v1s-p5-rb1.5-warm", 'q', v(1, 5, rb(1.5), true, true)),
        ("v1s-p5-gs-f32", 'f', v(1, 5, gs, false, true)),
        ("v1s-p5-rb-f32", 'f', v(1, 5, rb(1.0), false, true)),
        ("v1s-p4-rb1.3-f32", 'f', v(1, 4, rb(1.3), false, true)),
        ("v1s-p3-rb1.5-f32", 'f', v(1, 3, rb(1.5), false, true)),
        ("v1s-p2-rb1.5", 'q', v(1, 2, rb(1.5), false, true)),
        ("v1s-p2-rb1.5-f32", 'f', v(1, 2, rb(1.5), false, true)),
        ("v1s-p3-rb1.3", 'q', v(1, 3, rb(1.3), false, true)),
        ("v1s-p1-rb1.5", 'q', v(1, 1, rb(1.5), false, true)),
        ("v1s-p10-jac", 'q', v(1, 10, Sweep::Jacobi, false, true)),
        ("v1s-p8-jac", 'q', v(1, 8, Sweep::Jacobi, false, true)),
        ("v1s-p6-jac", 'q', v(1, 6, Sweep::Jacobi, false, true)),
        ("v1s-p4-jac", 'q', v(1, 4, Sweep::Jacobi, false, true)),
    ];
    for n in [24usize, 32, 48] {
        let dir = out.join(format!("{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, num, opts) in &rows {
            let path = dir.join(format!("{name}.f32"));
            match num {
                'q' => dump::<Q32>(&path, n, *opts, steps),
                'w' => dump::<lpc_engine::nodes::fluid::spike::Qw>(&path, n, *opts, steps),
                _ => dump::<f32>(&path, n, *opts, steps),
            }
        }
        eprintln!("grid {n}: {} variants", rows.len());
    }
}

fn dump<T: Num>(path: &std::path::Path, n: usize, opts: Opts, steps: u32) {
    let mut s = SpikeSolver::<T>::new(n, n, 10, 0.08, 3e-5, opts);
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let stride = s.stride();
    for k in 0..steps {
        scripted_step(&mut s, k);
        for field in [&s.r, &s.g, &s.b] {
            for j in 1..=n {
                for i in 1..=n {
                    let v = field[i + stride * j].to_q32().to_f32();
                    f.write_all(&v.to_le_bytes()).unwrap();
                }
            }
        }
    }
}
