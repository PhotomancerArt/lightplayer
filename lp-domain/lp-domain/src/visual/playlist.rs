//! [`Playlist`]: a sequenced list of Visual entries. Each entry plays
//! for `duration` seconds (or waits for cue if `None`); the playlist
//! cross-fades between entries through its single default transition.
//! See `docs/design/lightplayer/domain.md`.
//!
//! M3 deliberately omits per-entry transition overrides
//! (Q-D4); they are an additive future change.

use crate::binding::Binding;
use crate::schema::Artifact;
use crate::types::ArtifactSpec;
use crate::visual::transition_ref::TransitionRef;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// One entry in a Playlist. `duration: None` means "wait for cue".
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlaylistEntry {
    pub visual: ArtifactSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f32>,
    #[cfg_attr(
        feature = "schema-gen",
        schemars(schema_with = "super::params_table::toml_value_btree_map_schema")
    )]
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, toml::Value>,
}

/// Playlist behavior flags. M3 carries `loop` only; more land
/// additively (`shuffle`, `random_seed`, etc.).
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlaylistBehavior {
    #[serde(default, rename = "loop")]
    pub r#loop: bool,
}

/// Sequenced Visual entries with a single default transition.
///
/// # Example
///
/// ```text
/// schema_version = 1
/// title          = "Setlist"
///
/// [[entries]]
/// visual   = "../patterns/rainbow.pattern.toml"
/// duration = 60.0
///
/// [[entries]]
/// visual   = "../stacks/psychedelic.stack.toml"
/// duration = 90.0
///
/// [transition]
/// visual   = "../transitions/crossfade.transition.toml"
/// duration = 1.5
///
/// [behavior]
/// loop = true
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Playlist {
    pub schema_version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub entries: Vec<PlaylistEntry>,
    pub transition: TransitionRef,
    #[serde(default)]
    pub behavior: PlaylistBehavior,
    // TODO(binding-resolution): see Live::bindings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<String, Binding>,
}

impl Artifact for Playlist {
    const KIND: &'static str = "playlist";
    const CURRENT_VERSION: u32 = 1;

    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChannelName;

    #[test]
    fn playlist_loads_with_two_entries() {
        let p: Playlist = toml::from_str(setlist_toml()).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert!(p.behavior.r#loop);
    }

    #[test]
    fn playlist_round_trips() {
        let p: Playlist = toml::from_str(setlist_toml()).unwrap();
        let s = toml::to_string(&p).unwrap();
        let back: Playlist = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn entry_with_no_duration_is_wait_for_cue() {
        let p: Playlist = toml::from_str(
            r#"
            schema_version = 1
            title = "Cued"
            [[entries]]
            visual = "../patterns/rainbow.pattern.toml"
            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0
        "#,
        )
        .unwrap();
        assert!(p.entries[0].duration.is_none());
    }

    #[test]
    fn playlist_with_bindings_round_trips() {
        let toml = r#"
            schema_version = 1
            title = "Setlist"

            [[entries]]
            visual = "../patterns/rainbow.pattern.toml"
            duration = 10.0

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.0

            [bindings]
            "rainbow.pattern#params.speed" = { bus = "audio/in/0/level" }
        "#;
        let p: Playlist = toml::from_str(toml).unwrap();
        assert_eq!(p.bindings.len(), 1);
        let s = toml::to_string(&p).unwrap();
        let back: Playlist = toml::from_str(&s).unwrap();
        assert_eq!(p, back);
        assert_eq!(
            back.bindings.get("rainbow.pattern#params.speed"),
            Some(&Binding::Bus(ChannelName(String::from("audio/in/0/level"))))
        );
    }

    #[test]
    fn per_entry_transition_is_rejected() {
        let toml = r#"
            schema_version = 1
            title = "Setlist"
            [[entries]]
            visual = "../patterns/rainbow.pattern.toml"
            [entries.transition]
            visual = "../transitions/wipe.transition.toml"
            duration = 0.5
            [transition]
            visual = "../transitions/crossfade.transition.toml"
            duration = 1.0
        "#;
        let res: Result<Playlist, _> = toml::from_str(toml);
        assert!(
            res.is_err(),
            "Per-entry transition overrides are explicitly out of scope for M3"
        );
    }

    #[test]
    fn playlist_kind_constant() {
        assert_eq!(Playlist::KIND, "playlist");
        assert_eq!(Playlist::CURRENT_VERSION, 1);
    }

    fn setlist_toml() -> &'static str {
        r#"
            schema_version = 1
            title          = "Setlist"

            [[entries]]
            visual   = "../patterns/rainbow.pattern.toml"
            duration = 60.0

            [[entries]]
            visual   = "../patterns/fluid.pattern.toml"
            duration = 90.0

            [transition]
            visual   = "../transitions/crossfade.transition.toml"
            duration = 1.5

            [behavior]
            loop = true
        "#
    }
}
