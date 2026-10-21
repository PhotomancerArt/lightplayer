//! Which scope a touch set goes to, read off the board's binding graph.
//!
//! Plan D12 (rule 1): a picture is drawable when a `touches` reader exists in
//! the root scope or in the playing pattern's module scope, and the touches
//! go to the innermost scope that has one. The board already says all of
//! it: the binding-graph probe lists every bus channel per scope with its
//! consumers, and a scope only appears there while its module is loaded.
//!
//! "The playing pattern" is read as "a module the board has loaded": a
//! playlist loads only its playing entry (dormant entries are known by their
//! def alone), and a pattern opened on its own (`catalog/patterns/*`: a root
//! module around its `effect` module) plays its module all the time. So the
//! rule is: among the loaded module scopes with a `touches` reader, the
//! deepest one in the node tree, the root being the shallowest. A reader in
//! the playing pattern therefore wins over one at the root (Q13).
//!
//! Known edge: during a playlist crossfade both entries are loaded for the
//! fade's length, and a stroke started then may land in the outgoing one.
//! Sink scopes never list in the probe, so a bare (non-module) playlist entry
//! reading `touches` is not drawable — that is rule 2's road (P8).

use lpc_model::{NodeId, TOUCHES_CHANNEL};
use lpc_wire::{WireBindingGraph, WireScopeRef};

use super::UiDrawTarget;

/// The innermost loaded module scope with a `touches` reader, or `None`
/// when nothing reads touches. `depth_of` is a node's depth in the loaded
/// tree (the root module 0); a scope whose owner it does not know is not
/// considered.
pub fn touches_draw_target(
    graph: &WireBindingGraph,
    depth_of: impl Fn(NodeId) -> Option<usize>,
) -> Option<UiDrawTarget> {
    let mut best: Option<(usize, WireScopeRef)> = None;
    for channel in &graph.channels {
        if channel.name != TOUCHES_CHANNEL || channel.consumers.is_empty() {
            continue;
        }
        let Some(scope @ WireScopeRef::Module { owner }) = channel.scope else {
            continue;
        };
        let Some(depth) = depth_of(owner) else {
            continue;
        };
        if best.is_none_or(|(deepest, _)| depth > deepest) {
            best = Some((depth, scope));
        }
    }
    best.map(|(_, scope)| UiDrawTarget {
        scope,
        channel: TOUCHES_CHANNEL.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use lpc_model::Revision;
    use lpc_wire::WireBusChannel;

    use super::*;

    #[test]
    fn the_playing_patterns_reader_wins_over_the_roots() {
        let graph = graph(vec![
            channel(module(1), "touches", 1),
            channel(module(7), "touches", 2),
            channel(module(7), "speed", 1),
        ]);
        let target = touches_draw_target(&graph, depth).expect("drawable");
        assert_eq!(target.scope, module(7));
        assert_eq!(target.channel, "touches");
    }

    #[test]
    fn a_root_reader_alone_is_drawable_at_the_root() {
        let graph = graph(vec![channel(module(1), "touches", 1)]);
        let target = touches_draw_target(&graph, depth).expect("drawable");
        assert_eq!(target.scope, module(1));
    }

    #[test]
    fn no_reader_and_unknown_or_sink_scopes_are_not_drawable() {
        // A channel nobody reads (a stale panel writer's row), a sink, a
        // module the tree does not know, and a scope-less row.
        let mut sink = channel(module(1), "touches", 1);
        sink.scope = Some(WireScopeRef::Sink {
            owner: NodeId::new(3),
            entry: 1,
        });
        let mut scopeless = channel(module(1), "touches", 1);
        scopeless.scope = None;
        let graph = graph(vec![
            channel(module(1), "touches", 0),
            sink,
            channel(module(99), "touches", 1),
            scopeless,
        ]);
        assert_eq!(touches_draw_target(&graph, depth), None);
    }

    fn depth(owner: NodeId) -> Option<usize> {
        match owner.0 {
            1 => Some(0),
            7 => Some(2),
            _ => None,
        }
    }

    fn module(owner: u32) -> WireScopeRef {
        WireScopeRef::Module {
            owner: NodeId::new(owner),
        }
    }

    fn channel(scope: WireScopeRef, name: &str, readers: u32) -> WireBusChannel {
        WireBusChannel {
            scope: Some(scope),
            name: name.to_string(),
            kind: None,
            providers: Vec::new(),
            consumers: (0..readers).collect(),
            primary_visual: false,
        }
    }

    fn graph(channels: Vec<WireBusChannel>) -> WireBindingGraph {
        WireBindingGraph {
            revision: Revision::new(1),
            bindings: Vec::new(),
            channels,
        }
    }
}
