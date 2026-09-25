//! The committed traffic sample, and a test-only dictionary harvested from it.
//!
//! The dictionary is trained on the even-numbered lines, so the odd ones meet
//! it cold: every message class is covered, and what only the odd lines say
//! exercises inline keys and strings, back-references and codes alike. Values
//! over 48 bytes are left out: a repeated pixel payload is not dictionary
//! material, and the wire's dictionary would never hold one.
//!
//! The wire's real dictionary is generated from the wire types in `lpc-wire`;
//! this one exists only so the codec can be tested on real bytes.

use std::sync::OnceLock;

use lp_json_pack::{Dictionary, DictionaryBuilder};

/// One `M!` line of the sample.
pub struct FixtureLine {
    /// `<` board→host, `>` host→board.
    pub dir: u8,
    /// The JSON after `M!`.
    pub json: &'static [u8],
}

const SAMPLE: &[u8] = include_bytes!("../fixtures/choker-lens-sample.txt");

/// Every line of the sample, in order.
pub fn fixture_lines() -> &'static [FixtureLine] {
    static LINES: OnceLock<Vec<FixtureLine>> = OnceLock::new();
    LINES.get_or_init(|| {
        SAMPLE
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| {
                let (dir, rest) = (l[0], &l[2..]);
                assert!(dir == b'<' || dir == b'>', "bad direction");
                let json = rest.strip_prefix(b"M!").expect("an M! line");
                FixtureLine { dir, json }
            })
            .collect()
    })
}

/// The dictionary harvested from the sample's even-numbered lines.
pub fn fixture_dictionary() -> &'static Dictionary {
    static DICT: OnceLock<&'static Dictionary> = OnceLock::new();
    DICT.get_or_init(|| {
        let lines = fixture_lines();
        let mut b = DictionaryBuilder::new();
        for l in lines.iter().step_by(2) {
            b.observe_json(l.json);
        }
        let mut owned = b.build(2);
        owned.values.retain(|v| v.len() <= 48);
        let d = owned.leak();
        assert_eq!(d.check(), Ok(()));
        d
    })
}
