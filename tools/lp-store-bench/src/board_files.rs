//! The board's own files, which every store holds beside its projects
//! (shaped like the spike's `boardbase`: sizes, not contents, matter).

use std::sync::Arc;

use crate::CorpusDoc;

/// `/hardware.json` (~1.5 KB), `/.lp/{device,access,network,status-light}.json`
/// and `/lightplayer.json`, as absolute paths.
pub fn board_files() -> Vec<CorpusDoc> {
    let mut hw = String::from("{\n  \"board\": \"xiao-esp32c6\",\n  \"pins\": [\n");
    for i in 0..22 {
        hw.push_str(&format!(
            "    {{\n      \"gpio\": {i},\n      \"label\": \"D{i}\",\n      \"caps\": [\"out\", \"rmt\"]\n    }}{}\n",
            if i < 21 { "," } else { "" }
        ));
    }
    hw.push_str("  ]\n}\n");
    let files = [
        ("/hardware.json", hw),
        (
            "/.lp/device.json",
            "{\n  \"name\": \"lp-3f2a\",\n  \"uid\": \"dev0q4k2m7x9\"\n}\n".into(),
        ),
        (
            "/.lp/access.json",
            "{\n  \"version\": 3,\n  \"entries\": [\n    {\n      \"tier\": \"edit\",\n      \"key\": \"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08\"\n    }\n  ]\n}\n".into(),
        ),
        (
            "/.lp/network.json",
            "{\n  \"version\": 1,\n  \"networks\": [\n    {\n      \"ssid\": \"house\",\n      \"psk\": \"0123456789abcdef0123456789abcdef\"\n    }\n  ]\n}\n".into(),
        ),
        (
            "/.lp/status-light.json",
            "{\n  \"pin\": 15,\n  \"on\": true\n}\n".into(),
        ),
        (
            "/lightplayer.json",
            "{\n  \"active\": \"/projects/a\",\n  \"format\": 7\n}\n".into(),
        ),
    ];
    files
        .into_iter()
        .map(|(p, s)| CorpusDoc {
            rel: p.into(),
            bytes: Arc::new(s.into_bytes()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_json_is_about_one_and_a_half_kb() {
        let f = board_files();
        let hw = f.iter().find(|d| d.rel == "/hardware.json").unwrap();
        assert!((1200..2200).contains(&hw.bytes.len()), "{}", hw.bytes.len());
    }
}
