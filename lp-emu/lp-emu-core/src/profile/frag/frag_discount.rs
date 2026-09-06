//! Which call sites are dropped from a fragmentation replay, and why.
//!
//! `fw-emu` runs a permissive 256-resource board manifest, so a handful of
//! call sites allocate amounts no firmware ever will. Discounting them makes
//! a replay answer "what would this *workload* do to the classic" instead of
//! "what would the emulator's fixture board do". The matching lives here
//! rather than in the replay because three consumers need exactly the same
//! answer: the first-fit replay, the TLSF replay, and the counterfactual
//! transforms (an allocation the replay is going to drop must not be counted
//! into a scratch arena's size).

use ::alloc::string::String;
use ::alloc::vec::Vec;
use std::collections::HashMap;

use crate::profile::alloc::SymbolResolver;

/// Matches a call stack against the `--frag-discount-site` patterns, in the
/// order they were given.
pub(crate) struct DiscountMatcher<'a> {
    resolver: &'a SymbolResolver,
    patterns: Vec<String>,
    /// Keyed by call stack so the symbolizer runs once per distinct stack
    /// rather than once per allocation — traces carry millions of rows over a
    /// few thousand stacks.
    memo: HashMap<Vec<u32>, Option<usize>>,
}

impl<'a> DiscountMatcher<'a> {
    pub(crate) fn new(resolver: &'a SymbolResolver, patterns: &[String]) -> Self {
        Self {
            resolver,
            patterns: patterns.to_vec(),
            memo: HashMap::new(),
        }
    }

    /// Which pattern, if any, claims this call stack.
    ///
    /// Matched against the innermost non-infrastructure site — the string the
    /// pinning and would-OOM tables print — and, failing that, against every
    /// frame in the stack. The second half is not optional: `Vec` growth all
    /// reports the same site (`RawVecInner::finish_grow`, which the infra
    /// filter does not catch), so the two allocations worth discounting on
    /// this emulator — the `Vec<HwEndpoint>` behind
    /// `VirtualWs281xDriver::endpoints` and the manifest's `Vec<HwResource>` —
    /// are indistinguishable at the site alone and only tell apart one frame
    /// deeper. Matching the whole stack is also what lets a pattern name the
    /// *reason* an allocation exists rather than the allocator machinery that
    /// happened to make it.
    pub(crate) fn matches(&mut self, frames: &[u32]) -> Option<usize> {
        if self.patterns.is_empty() {
            return None;
        }
        if let Some(hit) = self.memo.get(frames) {
            return *hit;
        }
        let (site, _) = self.resolver.classify_alloc(frames);
        let resolver = self.resolver;
        let hit = self.patterns.iter().position(|pattern| {
            let pattern = pattern.as_str();
            site.contains(pattern)
                || frames.iter().any(|&addr| {
                    // Both spellings: the full demangled symbol, and the
                    // shortened one the report's call stacks print. A trait
                    // method's full name reads `<Type as Trait>::method`, so
                    // the `Type::method` a reader copies out of a report is a
                    // substring of the short form only.
                    resolver.resolve_full(addr).contains(pattern)
                        || resolver.resolve(addr).contains(pattern)
                })
        });
        self.memo.insert(frames.to_vec(), hit);
        hit
    }
}
