//! [`Figures`]: one test's observed figures, checked against its chip's
//! record — or, under a bless, written into it.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::record::{Record, Value};

/// The environment variable that turns [`Figures::verify`] from a check into
/// a rewrite. `just bless-chips` sets it; nothing else should.
pub const BLESS_ENV: &str = "LP_EMU_BLESS";

/// Where the records live, overridable for this crate's own tests.
const DIR_ENV: &str = "LP_EMU_FIGURES_DIR";

/// `lp-emu/esp/figures/<chip>.json` — inside the MIT fence, beside the chips.
pub fn record_path(chip: &str) -> PathBuf {
    let dir = std::env::var_os(DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../figures")));
    dir.join(format!("{chip}.json"))
}

/// One test's figures. Observe each with [`int`](Self::int),
/// [`string`](Self::string), [`text`](Self::text) or [`utf8`](Self::utf8),
/// then call [`verify`](Self::verify) once: it fails naming **every** figure
/// that moved, not just the first, because one firmware change usually moves
/// several at once.
///
/// Dropping a `Figures` without verifying it panics, so an observation can
/// never be silently unchecked.
#[must_use = "call .verify() — an unverified figure is an unchecked one"]
pub struct Figures {
    chip: String,
    test: String,
    observed: Vec<Observed>,
    verified: bool,
}

struct Observed {
    key: String,
    value: Value,
    /// See [`Figures::positional_int`].
    positional: bool,
}

impl Figures {
    /// `chip` names the record (`esp32c6`, `esp32v3`, `esp32s3`) and the bless
    /// command; `test` is only for the failure message.
    pub fn new(chip: &str, test: &str) -> Self {
        Self {
            chip: chip.to_owned(),
            test: test.to_owned(),
            observed: Vec::new(),
            verified: false,
        }
    }

    /// An integer figure.
    pub fn int<T>(&mut self, key: &str, value: T) -> &mut Self
    where
        T: TryInto<i64> + Copy + std::fmt::Debug,
    {
        let v = value
            .try_into()
            .unwrap_or_else(|_| panic!("figure `{key}`: {value:?} does not fit an i64"));
        self.observe(key, Value::Int(v), false)
    }

    /// An integer figure that depends on **where the build put the code** —
    /// a stack high-water, which is the deepest point an interrupt happened
    /// to land. Such a figure is reproducible within one build environment
    /// and not between two: the tree's image embeds the checkout's absolute
    /// paths and the host rustc's own build
    /// (`docs/debt/reference-images-are-not-reproducible-across-hosts.md`),
    /// so a desk worktree and a CI runner can read different values off the
    /// same commit.
    ///
    /// Checked exactly like [`int`](Self::int), against the record, which
    /// holds **CI's** value. A bless on a desk does not rewrite it (it would
    /// record a number CI cannot reproduce); a bless under
    /// `GITHUB_ACTIONS=true` does. A desk failure on one of these says so.
    pub fn positional_int<T>(&mut self, key: &str, value: T) -> &mut Self
    where
        T: TryInto<i64> + Copy + std::fmt::Debug,
    {
        let v = value
            .try_into()
            .unwrap_or_else(|_| panic!("figure `{key}`: {value:?} does not fit an i64"));
        self.observe(key, Value::Int(v), true)
    }

    /// A one-line string figure (a digest, a single line).
    pub fn string(&mut self, key: &str, value: &str) -> &mut Self {
        self.observe(key, Value::Str(value.to_owned()), false)
    }

    /// A text figure, recorded line by line so a moved line is a one-line
    /// diff in the record and a one-line report here.
    pub fn text(&mut self, key: &str, value: &str) -> &mut Self {
        self.observe(key, Value::text(value), false)
    }

    /// A byte-stream figure that must be text. Byte-for-byte as strict as a
    /// sha256 of the bytes: a stream that is not UTF-8 fails here rather than
    /// being compared lossily.
    pub fn utf8(&mut self, key: &str, bytes: &[u8]) -> &mut Self {
        let text = std::str::from_utf8(bytes).unwrap_or_else(|e| {
            panic!(
                "figure `{key}`: the stream is not UTF-8 ({e}), so it cannot be a text \
                 figure:\n{}",
                String::from_utf8_lossy(bytes)
            )
        });
        self.text(key, text)
    }

    /// The recorded value of an integer figure, for a test that needs it to
    /// build an expectation (a band's centre, an arithmetic identity). Under
    /// a bless there may be none yet; the caller's own figure check covers it.
    pub fn recorded_int(&self, key: &str) -> Option<i64> {
        match load(&self.chip).ok()?.get(key)? {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    fn observe(&mut self, key: &str, value: Value, positional: bool) -> &mut Self {
        if let Some(earlier) = self.observed.iter().find(|o| o.key == key) {
            assert_eq!(
                earlier.value, value,
                "figure `{key}` observed twice in one test with two values"
            );
            return self;
        }
        self.observed.push(Observed {
            key: key.to_owned(),
            value,
            positional,
        });
        self
    }

    /// Check every observed figure against the record: fail naming each one
    /// that moved (old → new) and the command that accepts them. Under
    /// [`BLESS_ENV`]`=1`, write the observed values into the record instead.
    pub fn verify(mut self) {
        self.verified = true;
        if blessing() {
            self.bless();
            return;
        }
        let record = load(&self.chip).unwrap_or_else(|e| panic!("{e}"));
        let mut positional_moved = false;
        let moved: Vec<String> = self
            .observed
            .iter()
            .filter_map(|o| {
                let line = describe_move(&o.key, record.get(&o.key), &o.value)?;
                positional_moved |= o.positional;
                Some(if o.positional {
                    format!("{line}   [positional]")
                } else {
                    line
                })
            })
            .collect();
        if moved.is_empty() {
            return;
        }
        let positional_note = if positional_moved {
            "\n[positional] figures depend on where the build put the code, so the record holds \
             CI's value and a desk build can legitimately read another. A desk bless leaves them \
             alone; take the new value from CI's failure (or bless under GITHUB_ACTIONS=true). \
             docs/chip-figures.md."
        } else {
            ""
        };
        panic!(
            "{n} pinned firmware figure{s} moved ({chip}, {test}):\n{list}\n\
             recorded in {path}\n\
             These are figures of the firmware IMAGE, not of the machine. If the firmware \
             changed on purpose, accept them with:\n\
             \n    just bless-chips {chip}\n\n\
             (on a pull request CI has already done that bless and posted the patch: \
             `just apply-ci-figures <pr>`) \
             and commit the record with the change that moved it. If only the emulator changed, \
             a moved figure is a finding: do not bless it.{positional_note}",
            n = moved.len(),
            s = if moved.len() == 1 { "" } else { "s" },
            chip = self.chip,
            test = self.test,
            list = moved.join("\n"),
            path = display_path(&self.chip),
        );
    }

    fn bless(&self) {
        let path = record_path(&self.chip);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .unwrap_or_else(|e| panic!("bless: open {}: {e}", path.display()));
        // Tests in one binary run in parallel and several read the same
        // figure: the read-modify-write is under the file's own lock.
        file.lock()
            .unwrap_or_else(|e| panic!("bless: lock {}: {e}", path.display()));
        let mut text = String::new();
        file.read_to_string(&mut text)
            .unwrap_or_else(|e| panic!("bless: read {}: {e}", path.display()));
        let mut record =
            Record::parse(&text).unwrap_or_else(|e| panic!("bless: {}: {e}", path.display()));
        let mut changed = false;
        for o in &self.observed {
            let Some(line) = describe_move(&o.key, record.get(&o.key), &o.value) else {
                continue;
            };
            if o.positional && !in_ci() {
                eprintln!(
                    "bless {}:{line}   [positional: NOT written on a desk — the record holds \
                     CI's value; see docs/chip-figures.md]",
                    self.chip
                );
                continue;
            }
            eprintln!("bless {}:{line}", self.chip);
            record.entries.insert(o.key.clone(), o.value.clone());
            changed = true;
        }
        if changed {
            let out = record.render();
            file.set_len(0).expect("bless: truncate");
            file.seek(SeekFrom::Start(0)).expect("bless: seek");
            file.write_all(out.as_bytes())
                .unwrap_or_else(|e| panic!("bless: write {}: {e}", path.display()));
        }
    }
}

impl Drop for Figures {
    fn drop(&mut self) {
        if !self.verified && !std::thread::panicking() {
            panic!(
                "figures observed by {} were never verified — call .verify()",
                self.test
            );
        }
    }
}

fn blessing() -> bool {
    std::env::var(BLESS_ENV).is_ok_and(|v| v == "1")
}

/// A CI runner — the build environment whose positional figures the record
/// holds.
fn in_ci() -> bool {
    std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true")
}

fn load(chip: &str) -> Result<Record, String> {
    let path = record_path(chip);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    Record::parse(&text).map_err(|e| format!("{}: {e}", display_path(chip)))
}

fn display_path(chip: &str) -> String {
    if std::env::var_os(DIR_ENV).is_some() {
        record_path(chip).display().to_string()
    } else {
        format!("lp-emu/esp/figures/{chip}.json")
    }
}

/// `None` when unmoved; otherwise one report line (or a few, for text).
fn describe_move(key: &str, was: Option<&Value>, now: &Value) -> Option<String> {
    let Some(was) = was else {
        return Some(format!("  {key}: not recorded → {now}"));
    };
    if was == now {
        return None;
    }
    Some(match (was, now) {
        (Value::Int(a), Value::Int(b)) => format!("  {key}: {a} → {b} ({:+})", b - a),
        (Value::Text(a), Value::Text(b)) => {
            let mut s = format!("  {key}: text moved ({} → {} lines)", a.len(), b.len());
            let differing: Vec<usize> = (0..a.len().max(b.len()))
                .filter(|&i| a.get(i) != b.get(i))
                .collect();
            for &i in differing.iter().take(6) {
                let show = |v: Option<&String>| v.map_or("<none>".to_owned(), |l| format!("{l:?}"));
                s.push_str(&format!(
                    "\n      line {}: {} → {}",
                    i + 1,
                    show(a.get(i)),
                    show(b.get(i))
                ));
            }
            if differing.len() > 6 {
                s.push_str(&format!("\n      … and {} more lines", differing.len() - 6));
            }
            s
        }
        (a, b) => format!("  {key}: {a} → {b}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_names_old_new_and_the_delta() {
        assert_eq!(
            describe_move("k", Some(&Value::Int(5)), &Value::Int(5)),
            None
        );
        assert_eq!(
            describe_move("k", Some(&Value::Int(45_344)), &Value::Int(45_328)).unwrap(),
            "  k: 45344 → 45328 (-16)"
        );
        assert_eq!(
            describe_move("k", None, &Value::Int(1)).unwrap(),
            "  k: not recorded → 1"
        );
        let moved = describe_move(
            "chain",
            Some(&Value::text("a\n[INIT] main stack 45344 B\nc\n")),
            &Value::text("a\n[INIT] main stack 45328 B\nc\n"),
        )
        .unwrap();
        assert_eq!(
            moved,
            "  chain: text moved (4 → 4 lines)\n      line 2: \"[INIT] main stack 45344 B\" → \
             \"[INIT] main stack 45328 B\""
        );
    }
}
