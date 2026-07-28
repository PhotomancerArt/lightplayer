//! Convert a mapping SVG (strict `path:N,count:N` subset) into a map2d
//! document on stdout.
//!
//! ```sh
//! cargo run -p lpc-mapping --example svg_to_map2d -- path/to/mapping.svg [sample_diameter]
//! ```

use lpc_mapping::DEFAULT_SAMPLE_DIAMETER;
use lpc_mapping::import::svg_to_doc;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: svg_to_map2d <mapping.svg> [sample_diameter]");
        std::process::exit(2);
    };
    let sample_diameter = args
        .next()
        .map(|raw| {
            raw.parse::<f32>()
                .expect("sample_diameter must be a number")
        })
        .unwrap_or(DEFAULT_SAMPLE_DIAMETER);
    let svg = std::fs::read_to_string(&path).expect("read svg");
    let doc = svg_to_doc(&svg, sample_diameter).expect("convert svg to map2d");
    println!("{}", doc.to_json_pretty());
}
