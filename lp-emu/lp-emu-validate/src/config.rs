//! `validate.toml`: the sets and the configuration table.
//!
//! Two things live here rather than in Rust, because they are *policy* and
//! change without a code change: which payloads make up a named set, and what
//! each configuration is trusted for. The payload registry itself stays in
//! Rust (`payload.rs`) — it carries regexes and field classes that a TOML file
//! could only hold as strings nobody checks.
//!
//! The file is compiled in with `include_str!`, so the runner works from any
//! directory; `ValidateConfig::load` reads an override path when a caller wants
//! one.

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::configuration::{Configuration, TrustTable};
use crate::payload::{Payload, find_payload};

const EMBEDDED: &str = include_str!("../validate.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct ValidateConfig {
    #[serde(default, rename = "set")]
    pub sets: Vec<PayloadSet>,
    #[serde(default, rename = "configuration")]
    pub configurations: Vec<ConfigurationEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PayloadSet {
    pub name: String,
    pub description: String,
    pub payloads: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigurationEntry {
    pub name: String,
    pub description: String,
    pub chip: String,
    #[serde(default)]
    pub trust: TrustTable,
}

impl ConfigurationEntry {
    pub fn parsed(&self) -> Result<Configuration> {
        Configuration::parse(&self.name)
    }
}

impl ValidateConfig {
    /// The table compiled into this binary.
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED).expect("the embedded validate.toml parses and validates")
    }

    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let cfg: Self = toml::from_str(text).context("parsing validate.toml")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        for set in &self.sets {
            if set.payloads.is_empty() {
                bail!("set `{}` lists no payloads", set.name);
            }
            for name in &set.payloads {
                find_payload(name).with_context(|| format!("in set `{}`", set.name))?;
            }
        }
        for c in &self.configurations {
            c.parsed()
                .with_context(|| format!("in configuration `{}`", c.name))?;
        }
        let mut names: Vec<&str> = self.sets.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        if names.len() != before {
            bail!("duplicate set name in validate.toml");
        }
        let mut names: Vec<&str> = self
            .configurations
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        if names.len() != before {
            bail!("duplicate configuration name in validate.toml");
        }
        Ok(())
    }

    pub fn set(&self, name: &str) -> Result<&PayloadSet> {
        match self.sets.iter().find(|s| s.name == name) {
            Some(s) => Ok(s),
            None => bail!(
                "unknown set `{name}` (known: {})",
                self.sets
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub fn payloads_in(&self, set: &str) -> Result<Vec<&'static Payload>> {
        self.set(set)?
            .payloads
            .iter()
            .map(|n| find_payload(n))
            .collect()
    }

    pub fn configuration(&self, name: &str) -> Result<&ConfigurationEntry> {
        match self.configurations.iter().find(|c| c.name == name) {
            Some(c) => Ok(c),
            None => bail!(
                "unknown configuration `{name}` (known: {})",
                self.configurations
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::{FieldClass, Grade};

    #[test]
    fn the_embedded_table_parses() {
        let cfg = ValidateConfig::embedded();
        assert!(!cfg.sets.is_empty());
        assert!(!cfg.configurations.is_empty());
    }

    #[test]
    fn every_set_names_known_payloads() {
        let cfg = ValidateConfig::embedded();
        for set in &cfg.sets {
            cfg.payloads_in(&set.name).unwrap();
        }
    }

    #[test]
    fn esp_emu_is_trusted_for_memory_and_not_for_time() {
        let cfg = ValidateConfig::embedded();
        let e = cfg.configuration("esp-emu:0.42.0").unwrap();
        assert_eq!(e.trust.grade(FieldClass::Memory), Grade::Measured);
        assert_eq!(e.trust.grade(FieldClass::Timing), Grade::Modeled);
        assert_eq!(e.trust.grade(FieldClass::UsbSerialJtag), Grade::Modeled);
        assert!(e.trust.because(FieldClass::Memory).is_some());
    }

    #[test]
    fn silicon_is_measured_everywhere_it_claims_anything() {
        let cfg = ValidateConfig::embedded();
        let s = cfg.configuration("silicon:esp32c6").unwrap();
        for class in [
            FieldClass::Memory,
            FieldClass::Timing,
            FieldClass::Pin,
            FieldClass::UsbSerialJtag,
            FieldClass::BootLog,
            FieldClass::Wire,
        ] {
            assert_eq!(
                s.trust.grade(class),
                Grade::Measured,
                "silicon should be measured for {class}"
            );
        }
    }

    #[test]
    fn a_set_naming_an_unknown_payload_is_refused() {
        let err = ValidateConfig::parse(
            r#"
[[set]]
name = "bad"
description = "x"
payloads = ["no-such-payload"]
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("no-such-payload"));
    }

    #[test]
    fn duplicate_set_names_are_refused() {
        let err = ValidateConfig::parse(
            r#"
[[set]]
name = "a"
description = "x"
payloads = ["gpio-calibrate"]

[[set]]
name = "a"
description = "y"
payloads = ["gpio-calibrate"]
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("duplicate set name"));
    }

    #[test]
    fn an_unparseable_configuration_name_is_refused() {
        let err = ValidateConfig::parse(
            r#"
[[configuration]]
name = "qemu:esp32c6"
description = "x"
chip = "esp32c6"
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("qemu"));
    }
}
