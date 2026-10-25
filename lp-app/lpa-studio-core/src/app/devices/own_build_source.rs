//! This Studio's own firmware builds, as a port the shell installs (DS5,
//! DS10): their facts at once (tiny — what every card's update standing is
//! read against), a build's bytes only when an update runs.
//!
//! **One build per target.** A Studio serves several targets (the C6, the
//! S3), and a board is offered the build of its own target, never another
//! target's ([`own_for`]). Holding only the first served build left every
//! board of another target with no update over the air at all (2026-10-24,
//! the S3 over Bluetooth).
//!
//! The served Studio's is
//! [`super::bundled_own_build::BundledOwnBuildSource`] (P8): it reads the
//! bundle's `ota-manifest.json` at start and fetches `core.z` / `engine.z`
//! (and slices the raw pieces out of the merged image) on
//! [`OwnBuildSource::load`]. Until a shell installs one, Studio has no build
//! of its own: no board is offered an update, and a board waiting for its
//! engine is still restored from the engine cache or
//! the firmware store (a heal's build is the board's own,
//! `HostBuild::for_heal`). [`MemoryOwnBuildSource`] serves tests and sims.
//!
//! **Verified on load.** The update host checks the loaded build's facts
//! against [`OwnBuildSource::facts`] before it offers a single byte: a
//! source whose bytes are not the build its manifest names is a load
//! failure, never an update.

use std::rc::Rc;

use lpa_update::{HostBuild, HostBuildFacts};

use crate::app::library::LocalBoxFuture;

/// Where this Studio's own builds come from.
pub trait OwnBuildSource {
    /// Every build's facts (identity, hashes, lengths), one per target;
    /// empty when this Studio carries no build of its own, or has not read
    /// them yet. The controller asks again after every device fold, so a
    /// source may learn them after it is installed.
    fn facts(&self) -> Vec<HostBuildFacts>;

    /// `target`'s build's bytes, loaded now. Called only when an update
    /// runs.
    fn load(&self, target: &str) -> LocalBoxFuture<'static, Result<HostBuild, String>>;
}

/// The build in `builds` for a board of `target`: none for a board whose
/// target is not known, or one this Studio holds no build of.
pub fn own_for<'a>(
    builds: &'a [HostBuildFacts],
    target: Option<&str>,
) -> Option<&'a HostBuildFacts> {
    let target = target?;
    builds.iter().find(|build| build.identity.target == target)
}

/// A build already in memory: tests, sims, hosts that hold it.
#[derive(Clone)]
pub struct MemoryOwnBuildSource {
    build: Rc<HostBuild>,
}

impl MemoryOwnBuildSource {
    pub fn new(build: HostBuild) -> Self {
        Self {
            build: Rc::new(build),
        }
    }
}

impl OwnBuildSource for MemoryOwnBuildSource {
    fn facts(&self) -> Vec<HostBuildFacts> {
        vec![self.build.facts()]
    }

    fn load(&self, target: &str) -> LocalBoxFuture<'static, Result<HostBuild, String>> {
        let loaded = match self.build.facts().identity.target == target {
            true => Ok(HostBuild::clone(&self.build)),
            false => Err(format!("this Studio has no build for {target}")),
        };
        Box::pin(core::future::ready(loaded))
    }
}

/// `loaded`, if it is the build `facts` names (the check the host makes
/// before it offers anything).
pub(crate) fn verified_own_build(
    loaded: Result<HostBuild, String>,
    facts: Option<&HostBuildFacts>,
) -> Result<HostBuild, String> {
    let build = loaded?;
    match facts {
        Some(facts) if build.facts() == *facts => Ok(build),
        Some(facts) => Err(format!(
            "the loaded build is not {} as its manifest says",
            facts.identity.build_id
        )),
        None => Err("this Studio has no build of its own".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use lpa_update::HostIdentity;

    use super::*;

    #[test]
    fn a_memory_source_states_its_facts_and_loads_the_same_build() {
        let build = build(1);
        let source = MemoryOwnBuildSource::new(build.clone());
        assert_eq!(source.facts(), vec![build.facts()]);
        let loaded = block_on(source.load("esp32c6-4mb"));
        assert_eq!(
            verified_own_build(loaded, source.facts().first()),
            Ok(build)
        );
        assert!(block_on(source.load("esp32s3-8mb")).is_err());
    }

    /// A board is offered its own target's build, never the first one
    /// served (the S3 over Bluetooth, 2026-10-24).
    #[test]
    fn a_board_gets_the_build_of_its_own_target() {
        let c6 = build(1).facts();
        let mut s3 = build(2).facts();
        s3.identity.target = "esp32s3-8mb".into();
        s3.identity.chip = "esp32s3".into();
        let builds = vec![c6.clone(), s3.clone()];
        assert_eq!(own_for(&builds, Some("esp32s3-8mb")), Some(&s3));
        assert_eq!(own_for(&builds, Some("esp32c6-4mb")), Some(&c6));
        assert_eq!(own_for(&builds, Some("esp32v3-4mb")), None);
        assert_eq!(own_for(&builds, None), None);
    }

    #[test]
    fn bytes_that_are_not_the_named_build_are_refused() {
        let named = build(1).facts();
        let other = build(2);
        assert!(verified_own_build(Ok(other), Some(&named)).is_err());
        assert!(verified_own_build(Ok(build(1)), None).is_err());
        assert_eq!(
            verified_own_build(Err("offline".to_string()), Some(&named)),
            Err("offline".to_string())
        );
    }

    fn build(fill: u8) -> HostBuild {
        HostBuild::from_parts(
            HostIdentity {
                target: "esp32c6-4mb".into(),
                chip: "esp32c6".into(),
                version: "2026.10.05-2".into(),
                build_id: "2026.10.05-2+626a1b851aaa".into(),
                wire_proto: 36,
                layout: 1,
                min_loader: 1,
            },
            vec![fill; 5000],
            vec![fill ^ 0xff; 9000],
            None,
            None,
        )
        .expect("a build")
    }

    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        let mut future = core::pin::pin!(future);
        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
        match future.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(value) => value,
            core::task::Poll::Pending => panic!("a memory build is immediately ready"),
        }
    }
}
