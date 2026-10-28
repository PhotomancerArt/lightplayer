//! What publishing the Graph's offers costs (mapping rebuild M4 P07): the
//! subject, port and choice lists every verb that takes values carries,
//! rebuilt on every view while the Graph is open.
//!
//! A measurement, not a gate (wall-clock time is never asserted): run it
//! in a release build,
//!
//! ```text
//! cargo test -p lpa-studio-core --release graph_offer_cost -- --ignored --nocapture
//! ```
//!
//! It opens the repo's biggest project, the radiance dome (`lab/mapping/
//! projects/radiance-dome`), as Studio's Graph opens it (the files crate,
//! port tables from the engine's own walk), and times
//! [`publish_graph_offers`] over and over on one editor (the project's
//! patch is worked out once, as the view needs it anyway), then what a view
//! rebuilt while nothing changed costs now that the controller keeps the
//! verbs that take values (`GraphController::publish_offers`).

use std::path::Path;
use std::time::Instant;

use lab_mapping_files::{DirFiles, Opened};
use lab_mapping_model::{Editor, Target};

use super::graph_offers::{publish_graph_offers, publish_key_offers, value_offers};
use super::graph_port_tables::port_tables;
use crate::{OfferParamKind, UiOfferTree};

#[test]
#[ignore = "a measurement: run in release with --ignored --nocapture (module docs)"]
fn graph_offer_cost_on_the_radiance_dome() {
    for dir in [
        "lab/mapping/projects/radiance-dome",
        "catalog/projects/zook-dome",
        "projects/test/circular-display-6",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(dir);
        let Opened { mut project, files } =
            Opened::open(&DirFiles::new(&path)).unwrap_or_else(|e| panic!("{dir}: {e}"));
        project.set_port_tables(port_tables(&project, &files));
        let mut editor = Editor::new(project);
        measure(dir, "nothing selected", &editor);
        let first = editor.project.placed_segments()[0].1.id.clone();
        editor.select(vec![Target::Generated(first)]);
        measure(dir, "a segment selected", &editor);
    }
}

fn measure(dir: &str, what: &str, editor: &Editor) {
    let publish = || {
        let mut tree = UiOfferTree::new();
        publish_graph_offers(&mut tree, editor);
        tree
    };
    // The project's patch, worked out once (the view needs it anyway).
    let tree = publish();
    let options: usize = tree
        .iter()
        .flat_map(|offer| offer.params())
        .map(|p| match &p.kind {
            OfferParamKind::Choice { options, .. } => options.len(),
            _ => 0,
        })
        .sum();
    let mut times: Vec<f64> = (0..30)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(publish());
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    // A view rebuilt while nothing changed: the key verbs worked out, the
    // verbs that take values as the controller kept them.
    let kept = value_offers(editor);
    let mut reused: Vec<f64> = (0..30)
        .map(|_| {
            let start = Instant::now();
            let mut tree = UiOfferTree::new();
            publish_key_offers(&mut tree, editor);
            for offer in &kept {
                tree.publish(offer.clone());
            }
            std::hint::black_box(tree);
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    reused.sort_by(f64::total_cmp);
    println!(
        "{dir} · {what}: {} offers, {options} options; worked out: min {:.2} ms, median {:.2} ms, max {:.2} ms; kept: median {:.2} ms, max {:.2} ms ({} lamps)",
        tree.len(),
        times[0],
        times[times.len() / 2],
        times[times.len() - 1],
        reused[reused.len() / 2],
        reused[reused.len() - 1],
        editor.project.lamp_count(),
    );
}
