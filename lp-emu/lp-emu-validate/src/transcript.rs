//! Transcripts: committed, verbatim, never edited.
//!
//! A transcript is the raw bytes a payload produced on a configuration, stored
//! under `lp-emu/transcripts/<chip>/<payload>/<configuration>-<date>-<short>.txt`
//! with a `<same>.meta.json` sidecar carrying the header.
//!
//! The loader does exactly two normalisations, and both are framing rather than
//! content: it splits on `\n` and drops a trailing `\r`. ANSI escapes, espflash
//! progress bars, ROM banners and every digit stay exactly as captured —
//! masking happens later, per comparison, so the same file can be read with
//! different rules without ever being rewritten.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::configuration::Configuration;
use crate::header::{InbandHeader, TranscriptHeader};
use crate::mask::MaskSet;
use crate::payload::{Payload, SeriesSpec, find_payload};

/// The prefix `fw-checks::emit_record_json` prints structured records behind.
pub const RECORD_PREFIX: &str = "[fw-check-json] ";

/// One structured record.
#[derive(Clone, Debug)]
pub struct Record {
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
    /// 1-based line number in the transcript.
    pub line: usize,
}

impl Record {
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.fields.get(field)
    }
}

/// One indexed sample of a series, keyed by the series' key capture.
#[derive(Clone, Debug)]
pub struct SeriesSample {
    pub key: String,
    pub values: BTreeMap<String, String>,
    pub line: usize,
}

#[derive(Debug)]
pub struct Transcript {
    pub path: Option<PathBuf>,
    pub header: TranscriptHeader,
    pub payload: &'static Payload,
    pub configuration: Configuration,
    /// Lines as captured, `\r` stripped. Nothing else is touched.
    pub lines: Vec<String>,
}

impl Transcript {
    /// Load `<path>` plus its `<path>.meta.json` sidecar.
    ///
    /// The sidecar is required: a transcript with no provenance is not a
    /// transcript. If the body also carries an in-band header, the two must
    /// agree.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let sidecar = sidecar_path(path);
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("reading transcript {}", path.display()))?;
        let meta = std::fs::read_to_string(&sidecar).with_context(|| {
            format!(
                "reading transcript sidecar {} — every committed transcript needs one \
                 (see lp-emu/lp-emu-validate/README.md)",
                sidecar.display()
            )
        })?;
        let header = TranscriptHeader::from_json(&meta)
            .with_context(|| format!("in {}", sidecar.display()))?;
        let mut t = Self::from_parts(header, &body)?;
        t.path = Some(path.to_path_buf());
        Ok(t)
    }

    pub fn from_parts(header: TranscriptHeader, body: &str) -> Result<Self> {
        header.validate()?;
        let payload = find_payload(&header.payload)?;
        let configuration = header.configuration()?;
        let lines: Vec<String> = body
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect();
        let t = Self {
            path: None,
            header,
            payload,
            configuration,
            lines,
        };
        if let Some(inband) = t.inband_header()? {
            t.header.agrees_with_inband(&inband)?;
        }
        Ok(t)
    }

    /// The in-band header line, if the payload printed one.
    pub fn inband_header(&self) -> Result<Option<InbandHeader>> {
        for line in &self.lines {
            if let Some(h) = InbandHeader::from_line(line)? {
                return Ok(Some(h));
            }
        }
        Ok(None)
    }

    /// 1-based line number of the payload's sentinel, if it appeared.
    pub fn sentinel_line(&self) -> Option<usize> {
        let marker = self.payload.sentinel.marker();
        self.lines
            .iter()
            .position(|l| l.contains(marker))
            .map(|i| i + 1)
    }

    /// Every `[fw-check-json] ` record, in order.
    pub fn records(&self) -> Result<Vec<Record>> {
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let Some(idx) = line.find(RECORD_PREFIX) else {
                continue;
            };
            let json = strip_ansi(line[idx + RECORD_PREFIX.len()..].trim());
            let value: Value = serde_json::from_str(&json)
                .with_context(|| format!("parsing record on line {}: {json}", i + 1))?;
            let Value::Object(map) = value else {
                bail!("record on line {} is not a JSON object: {json}", i + 1);
            };
            let kind = map
                .get("kind")
                .and_then(Value::as_str)
                .with_context(|| format!("record on line {} has no `kind`", i + 1))?
                .to_string();
            out.push(Record {
                kind,
                fields: map.into_iter().collect(),
                line: i + 1,
            });
        }
        Ok(out)
    }

    /// The records of one kind.
    pub fn records_of(&self, kind: &str) -> Result<Vec<Record>> {
        Ok(self
            .records()?
            .into_iter()
            .filter(|r| r.kind == kind)
            .collect())
    }

    /// Every match of a series, keyed by the spec's key capture.
    ///
    /// A repeated key wins last-writes: the calibration payload reports the
    /// same gpio many times and only the final state is comparable.
    pub fn series(&self, spec: &SeriesSpec) -> Vec<SeriesSample> {
        let re = spec.regex();
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let stripped = strip_ansi(line);
            let Some(caps) = re.captures(&stripped) else {
                continue;
            };
            let key = caps[spec.key].to_string();
            let mut values = BTreeMap::new();
            for (field, _) in spec.fields {
                if let Some(m) = caps.name(field) {
                    values.insert((*field).to_string(), m.as_str().to_string());
                }
            }
            out.push(SeriesSample {
                key,
                values,
                line: i + 1,
            });
        }
        out
    }

    /// The transcript with a mask set applied, line by line.
    ///
    /// This is the human view — `mask-transcript.sh` in code. The replay's
    /// verdict comes from records and series, not from this.
    pub fn masked(&self, set: &MaskSet) -> Vec<String> {
        self.lines.iter().map(|l| set.apply(l)).collect()
    }
}

/// `foo.txt` -> `foo.txt.meta.json`.
pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".meta.json");
    PathBuf::from(s)
}

fn strip_ansi(s: &str) -> String {
    crate::mask::ANSI.apply(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::HEADER_SCHEMA;
    use crate::payload::COMPILE_TICK;

    fn header(payload: &str) -> TranscriptHeader {
        TranscriptHeader {
            schema: HEADER_SCHEMA,
            payload: payload.into(),
            chip: "esp32c6".into(),
            configuration: "esp-emu:0.42.0".into(),
            date: "2026-09-06".into(),
            firmware_commit: "d6cfaa2051ae".into(),
            firmware_features: vec!["esp32c6".into()],
            firmware_dirty: None,
            silicon_rev: None,
            board: None,
            mac: None,
            tools: BTreeMap::new(),
            source: None,
            capture: None,
            note: None,
            trust: Default::default(),
        }
    }

    #[test]
    fn parses_records_through_ansi_and_cr() {
        let body = "\u{1b}[0;32m[INFO] fw_checks: [fw-check-json] \
                    {\"kind\":\"case-summary\",\"peak_used\":48132}\u{1b}[0m\r\n\
                    noise\r\n";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        let records = t.records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, "case-summary");
        assert_eq!(records[0].get("peak_used").unwrap().as_u64(), Some(48132));
        assert_eq!(records[0].line, 1);
    }

    #[test]
    fn a_record_without_a_kind_is_an_error() {
        let body = "[fw-check-json] {\"peak_used\":1}\n";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        let err = t.records().unwrap_err().to_string();
        assert!(err.contains("`kind`"), "{err}");
    }

    #[test]
    fn malformed_record_json_names_its_line() {
        let body = "ok\n[fw-check-json] {not json}\n";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        let err = format!("{:#}", t.records().unwrap_err());
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn series_extraction_indexes_by_key() {
        let body = "\
[inc-shader-compile] case=examples-basic tick=1 stage= slice_cycles=93993 slice_us=587 mem_before=321600 free/3936 used mem_after=308508 free/17028 used
[inc-shader-compile] case=examples-basic tick=2 stage=AssembleModule slice_cycles=130497 slice_us=815 mem_before=308508 free/17028 used mem_after=304620 free/20916 used
";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        let samples = t.series(&COMPILE_TICK);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].key, "1");
        assert_eq!(samples[0].values["mem_after_used"], "17028");
        assert_eq!(samples[1].values["stage"], "AssembleModule");
    }

    #[test]
    fn sentinel_is_found_by_line() {
        let body = "a\nb\n[inc-shader-compile] === DONE ===\n";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        assert_eq!(t.sentinel_line(), Some(3));

        let t = Transcript::from_parts(header("shader-compile-stress"), "a\nb\n").unwrap();
        assert_eq!(t.sentinel_line(), None);
    }

    #[test]
    fn an_inband_header_that_contradicts_the_sidecar_is_refused() {
        let inband = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: "gpio-calibrate".into(),
            chip: "esp32c6".into(),
            firmware_commit: "d6cfaa2051ae".into(),
            firmware_features: "esp32c6".into(),
            firmware_dirty: None,
        };
        let body = format!("{}\n", inband.to_line());
        let err = Transcript::from_parts(header("shader-compile-stress"), &body)
            .unwrap_err()
            .to_string();
        assert!(err.contains("payload"), "{err}");
    }

    #[test]
    fn an_agreeing_inband_header_is_accepted() {
        let inband = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: "shader-compile-stress".into(),
            chip: "esp32c6".into(),
            firmware_commit: "d6cfaa205".into(),
            firmware_features: "esp32c6".into(),
            firmware_dirty: None,
        };
        let body = format!("{}\n", inband.to_line());
        let t = Transcript::from_parts(header("shader-compile-stress"), &body).unwrap();
        assert!(t.inband_header().unwrap().is_some());
    }

    #[test]
    fn sidecar_path_appends() {
        assert_eq!(
            sidecar_path(Path::new("a/b/c.txt")),
            PathBuf::from("a/b/c.txt.meta.json")
        );
    }

    #[test]
    fn masked_view_is_a_view_not_an_edit() {
        let body = "\u{1b}[0;32mI (23) boot: x\u{1b}[0m\n";
        let t = Transcript::from_parts(header("shader-compile-stress"), body).unwrap();
        assert_eq!(t.masked(&crate::mask::COMPILE_HARNESS)[0], "I (N) boot: x");
        assert!(
            t.lines[0].contains('\u{1b}'),
            "the loaded lines are untouched"
        );
    }
}
