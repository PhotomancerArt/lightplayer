//! The mapping test corpus: four authored archetypes plus the real fyeah
//! sign, shared by resolver tests, Studio stories, and editor fixtures.
//!
//! The JSON documents are the canonical corpus (they exercise serde on every
//! use); fyeah derives from its real mapping SVG through the importer so the
//! conversion path is exercised too. The strings are tiny and only linked
//! into binaries that reference them.

use crate::import::svg_to_doc;
use crate::map2d_doc::{DEFAULT_SAMPLE_DIAMETER, Map2dDoc};

/// One multi-ring button: 16-lamp outer ring + derived 8-lamp inner ring.
pub const BASIC_BUTTON_JSON: &str = include_str!("corpus/basic_button.map2d.json");

/// Linear art: two ear paths + a headband path, 48 lamps.
pub const CAT_EARS_JSON: &str = include_str!("corpus/cat_ears.map2d.json");

/// A 16×16 snake-routed panel, 256 lamps across 2 universes.
pub const PANEL_16X16_JSON: &str = include_str!("corpus/panel_16x16.map2d.json");

/// One physical channel that leaves the lit run, jumpers across on inert
/// wire, and comes back: the format-2 archetype (24 lamps, one object).
pub const GAPPED_PATH_JSON: &str = include_str!("corpus/gapped_path.map2d.json");

/// The real fyeah sign mapping SVG (10 labeled paths, 219 lamps).
pub const FYEAH_SVG: &str = include_str!("corpus/fyeah_mapping.svg");

pub fn basic_button() -> Map2dDoc {
    Map2dDoc::from_json(BASIC_BUTTON_JSON).expect("corpus basic_button parses")
}

pub fn cat_ears() -> Map2dDoc {
    Map2dDoc::from_json(CAT_EARS_JSON).expect("corpus cat_ears parses")
}

pub fn panel_16x16() -> Map2dDoc {
    Map2dDoc::from_json(PANEL_16X16_JSON).expect("corpus panel_16x16 parses")
}

pub fn gapped_path() -> Map2dDoc {
    Map2dDoc::from_json(GAPPED_PATH_JSON).expect("corpus gapped_path parses")
}

pub fn fyeah() -> Map2dDoc {
    svg_to_doc(FYEAH_SVG, DEFAULT_SAMPLE_DIAMETER).expect("corpus fyeah imports")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map2d_resolve::{LAMPS_PER_UNIVERSE, resolve};

    /// Expected `path:N,count:N` labels from the fyeah mapping SVG.
    const FYEAH_COUNTS: [u32; 10] = [23, 25, 25, 27, 21, 26, 30, 29, 7, 6];

    #[test]
    fn basic_button_resolves_to_two_rings() {
        let resolved = resolve(&basic_button()).unwrap();
        assert_eq!(resolved.lamps.len(), 24);
        assert_eq!(resolved.universe_count(), 1);
    }

    #[test]
    fn cat_ears_resolves_three_paths() {
        let resolved = resolve(&cat_ears()).unwrap();
        assert_eq!(resolved.lamps.len(), 48);
        assert_eq!(resolved.spans.len(), 3);
        assert_eq!(resolved.spans[2].start, 26);
        assert_eq!(resolved.spans[2].count, 22);
    }

    #[test]
    fn panel_resolves_256_lamps_across_two_universes() {
        let resolved = resolve(&panel_16x16()).unwrap();
        assert_eq!(resolved.lamps.len(), 256);
        assert_eq!(resolved.universe_count(), 2);
        // Snake: row 0 runs +x, row 1 runs -x.
        assert_eq!(resolved.lamps[0].pos, [100.0, 80.0]);
        assert_eq!(resolved.lamps[15].pos, [100.0 + 15.0 * 26.0, 80.0]);
        assert_eq!(resolved.lamps[16].pos, [100.0 + 15.0 * 26.0, 106.0]);
    }

    /// The format-2 archetype: 24 lamps on two lit runs, none on the jumper
    /// between them, and the whole channel still one object.
    #[test]
    fn gapped_path_lights_both_runs_and_never_the_jumper() {
        let doc = gapped_path();
        assert_eq!(doc.format, 2);
        assert_eq!(doc.required_format(), 2);

        let resolved = resolve(&doc).unwrap();
        assert_eq!(resolved.lamps.len(), 24);
        assert_eq!(resolved.spans.len(), 1);
        // The jumper runs along y = 100 between x = 100 and x = 160; every
        // lamp sits on one of the two vertical runs, none in between.
        for lamp in &resolved.lamps {
            assert!(
                lamp.pos[0] == 100.0 || lamp.pos[0] == 160.0,
                "lamp {} landed on the jumper at {:?}",
                lamp.index,
                lamp.pos
            );
        }
        // Even split, and the pitch is continuous across the jumper.
        let on_first = resolved
            .lamps
            .iter()
            .filter(|lamp| lamp.pos[0] == 100.0)
            .count();
        assert_eq!(on_first, 12);
    }

    #[test]
    fn fyeah_imports_all_labeled_paths() {
        let doc = fyeah();
        assert_eq!(doc.objects.len(), FYEAH_COUNTS.len());
        assert_eq!(doc.canvas, Some([0.0, 0.0, 2146.8, 453.5]));

        let resolved = resolve(&doc).unwrap();
        let total: u32 = FYEAH_COUNTS.iter().sum();
        assert_eq!(total, 219);
        assert_eq!(resolved.lamps.len(), total as usize);
        for (span, expected) in resolved.spans.iter().zip(FYEAH_COUNTS) {
            assert_eq!(span.count, expected);
        }
        assert_eq!(resolved.universe_count(), 2);

        // The universe boundary lands mid-object (inside p7).
        let boundary = &resolved.lamps[LAMPS_PER_UNIVERSE as usize];
        assert_eq!(boundary.address.universe, 1);
        assert_eq!(boundary.address.channel, 0);
        assert_eq!(boundary.object, 6);
    }

    #[test]
    fn corpus_documents_fit_the_asset_body_budget() {
        // Studio applies mapping docs as whole asset bodies with a 10 KiB
        // client-side budget (lpa-studio-core MAX_ASSET_BODY_BYTES). The
        // corpus — including the imported real sign — must stay well inside.
        const MAX_ASSET_BODY_BYTES: usize = 10 * 1024;
        for (name, doc) in [
            ("basic_button", basic_button()),
            ("cat_ears", cat_ears()),
            ("panel_16x16", panel_16x16()),
            ("gapped_path", gapped_path()),
            ("fyeah", fyeah()),
        ] {
            let bytes = doc.to_json().len();
            assert!(
                bytes < MAX_ASSET_BODY_BYTES,
                "{name} serializes to {bytes} bytes, over the {MAX_ASSET_BODY_BYTES} budget"
            );
        }
    }

    #[test]
    fn corpus_documents_round_trip() {
        for doc in [
            basic_button(),
            cat_ears(),
            panel_16x16(),
            gapped_path(),
            fyeah(),
        ] {
            let round_tripped = Map2dDoc::from_json(&doc.to_json()).unwrap();
            assert_eq!(round_tripped, doc);
            assert_eq!(resolve(&round_tripped).unwrap(), resolve(&doc).unwrap());
        }
    }
}
