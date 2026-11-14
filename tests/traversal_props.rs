// SPDX-License-Identifier: MIT
//! Property tests: the traversals must agree with each other on random graphs.

use std::collections::HashSet;

use chrono::Utc;
use mem_graph::{Entity, EntityId, GraphStore, Relationship};
use proptest::prelude::*;

/// A random directed graph: `n` nodes named "0".."n-1" and up to 80 edges
/// (self-loops, cycles and parallel edges with different types included).
fn graph() -> impl Strategy<Value = (usize, Vec<(usize, usize, u8)>)> {
    (1usize..=30).prop_flat_map(|n| {
        let edges = prop::collection::vec((0..n, 0..n, 0u8..2), 0..80);
        (Just(n), edges)
    })
}

fn build(n: usize, edges: &[(usize, usize, u8)]) -> GraphStore {
    let mut g = GraphStore::new();
    for i in 0..n {
        g.add_entity(Entity::new(EntityId::new(i.to_string()), "N")).unwrap();
    }
    for &(from, to, kind) in edges {
        let rel = Relationship::new(EntityId::new(from.to_string()), EntityId::new(to.to_string()), format!("r{kind}"));
        // Duplicate triples are rejected; that is fine for these tests.
        let _ = g.add_relationship(rel);
    }
    g
}

fn as_set(ids: &[EntityId]) -> HashSet<EntityId> {
    ids.iter().cloned().collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// DFS and BFS visit exactly the same nodes for every start and depth
    /// limit, and DFS reports each node once, starting with `start`.
    #[test]
    fn dfs_and_bfs_reach_the_same_set((n, edges) in graph(), start in 0usize..30, depth in 0usize..=6) {
        let g = build(n, &edges);
        let start = EntityId::new((start % n).to_string());
        let dfs = g.dfs(&start, depth).unwrap();
        let bfs = g.bfs(&start, depth).unwrap();

        prop_assert_eq!(&dfs[0], &start);
        prop_assert_eq!(dfs.len(), as_set(&dfs).len(), "dfs returned a node twice: {:?}", dfs);
        prop_assert_eq!(as_set(&dfs), as_set(&bfs));
    }

    /// Every node BFS finds within `depth` hops has a shortest path of at
    /// most `depth` hops, and the unlimited traversals equal the closure.
    #[test]
    fn traversals_agree_with_shortest_path_and_closure((n, edges) in graph(), start in 0usize..30, depth in 0usize..=6) {
        let g = build(n, &edges);
        let start = EntityId::new((start % n).to_string());

        for id in g.bfs(&start, depth).unwrap() {
            let path = g.shortest_path(&start, &id).unwrap();
            prop_assert!(path.is_some_and(|p| p.len() - 1 <= depth));
        }

        let closure = g.transitive_closure(&start).unwrap();
        prop_assert_eq!(&as_set(&g.dfs(&start, usize::MAX).unwrap()), &closure);
        prop_assert_eq!(&as_set(&g.bfs(&start, usize::MAX).unwrap()), &closure);
    }

    /// With no validity windows set, the point-in-time queries match the
    /// plain ones exactly (same order too).
    #[test]
    fn at_queries_match_plain_ones_without_windows((n, edges) in graph(), start in 0usize..30, depth in 0usize..=6) {
        let g = build(n, &edges);
        let start = EntityId::new((start % n).to_string());
        let now = Utc::now();
        prop_assert_eq!(g.bfs_at(&start, depth, now).unwrap(), g.bfs(&start, depth).unwrap());
        let goal = EntityId::new((n - 1).to_string());
        let plain = g.shortest_path(&start, &goal).unwrap().map(|p| p.len());
        let at = g.shortest_path_at(&start, &goal, now).unwrap().map(|p| p.len());
        prop_assert_eq!(plain, at);
    }
}

/// The old recursive DFS used one stack frame per hop and overflowed on deep
/// graphs. Test threads have a small stack (2 MiB by default), so this would
/// abort the test binary if `dfs` recursed.
#[test]
fn dfs_on_a_200k_node_chain_does_not_overflow_the_stack() {
    const N: usize = 200_000;
    let mut g = GraphStore::new();
    for i in 0..N {
        g.add_entity(Entity::new(EntityId::new(i.to_string()), "N")).unwrap();
    }
    for i in 1..N {
        g.add_relationship(Relationship::new(
            EntityId::new((i - 1).to_string()),
            EntityId::new(i.to_string()),
            "next",
        ))
        .unwrap();
    }

    let all = g.dfs(&EntityId::new("0"), usize::MAX).unwrap();
    assert_eq!(all.len(), N);
    assert_eq!(all[N - 1].as_str(), (N - 1).to_string());

    let limited = g.dfs(&EntityId::new("0"), 1_000).unwrap();
    assert_eq!(limited.len(), 1_001);
}
