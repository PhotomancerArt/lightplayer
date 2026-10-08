//! Corpora: a directory of files read into (relative path, bytes), or a small
//! synthetic one built in code (what the unit tests use — they never read the
//! planning dir).

use std::path::Path;
use std::sync::Arc;

/// One document of a corpus: path relative to the project root, and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusDoc {
    pub rel: String,
    pub bytes: Arc<Vec<u8>>,
}

/// A project's documents, sorted by path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    pub name: String,
    pub docs: Vec<CorpusDoc>,
}

impl Corpus {
    /// Read every file under `dir` (skipping `.DS_Store`), sorted by path.
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let mut docs = Vec::new();
        walk(dir, "", &mut docs)?;
        docs.sort_by(|a, b| a.rel.cmp(&b.rel));
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Self { name, docs })
    }

    /// A deterministic synthetic project: `modules` patterns, each a shader of
    /// `shader_len`-ish bytes plus two small JSON docs, and three top-level docs.
    pub fn synthetic(name: &str, modules: usize, shader_len: usize, seed: u64) -> Self {
        let mut rng = lp_nor_sim::SimRng::new(seed);
        let mut docs = Vec::new();
        let mut push = |rel: String, bytes: Vec<u8>| {
            docs.push(CorpusDoc {
                rel,
                bytes: Arc::new(bytes),
            })
        };
        push(
            "project.json".into(),
            pretty(&format!(
                "{{\n  \"name\": \"{name}\",\n  \"format\": 7,\n  \"modules\": {modules}\n}}\n"
            )),
        );
        push(
            "playlist.json".into(),
            pretty("{\n  \"entries\": {\n    \"0\": \"m0\"\n  }\n}\n"),
        );
        push(
            "output.json".into(),
            pretty("{\n  \"pin\": 10,\n  \"leds\": 73\n}\n"),
        );
        const WORDS: [&str; 12] = [
            "float", "vec3", "uniform", "time", "speed", "palette", "mix", "sin", "uv", "return",
            "phase", "color",
        ];
        for m in 0..modules {
            let mut glsl = String::new();
            let len = shader_len / 2 + rng.below(shader_len as u64 + 1) as usize;
            while glsl.len() < len {
                glsl.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
                glsl.push(if rng.chance(1, 8) { '\n' } else { ' ' });
            }
            glsl.push('\n');
            push(format!("modules/m{m}/shader.glsl"), glsl.into_bytes());
            push(
                format!("modules/m{m}/shader.json"),
                pretty(&format!(
                    "{{\n  \"bindings\": {{\n    \"speed\": {}\n  }}\n}}\n",
                    rng.below(100)
                )),
            );
            push(
                format!("modules/m{m}/module.json"),
                pretty(&format!("{{\n  \"kind\": \"shader\",\n  \"id\": {m}\n}}\n")),
            );
        }
        docs.sort_by(|a, b| a.rel.cmp(&b.rel));
        Self {
            name: name.into(),
            docs,
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.docs.iter().map(|d| d.bytes.len()).sum()
    }
}

fn pretty(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

fn walk(root: &Path, rel: &str, out: &mut Vec<CorpusDoc>) -> std::io::Result<()> {
    for e in std::fs::read_dir(root.join(rel))? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".DS_Store" {
            continue;
        }
        let r = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if e.file_type()?.is_dir() {
            walk(root, &r, out)?;
        } else {
            out.push(CorpusDoc {
                rel: r,
                bytes: Arc::new(std::fs::read(e.path())?),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_is_deterministic_and_sorted() {
        let a = Corpus::synthetic("s", 4, 400, 1);
        let b = Corpus::synthetic("s", 4, 400, 1);
        assert_eq!(a, b);
        assert_eq!(a.docs.len(), 3 + 4 * 3);
        assert!(a.docs.windows(2).all(|w| w[0].rel < w[1].rel));
    }
}
