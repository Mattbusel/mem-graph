// SPDX-License-Identifier: MIT
use criterion::{criterion_group, criterion_main, Criterion};
use mem_graph::{Entity, EntityId, GraphStore, Relationship};

fn build_chain(n: usize) -> GraphStore {
    let mut g = GraphStore::new();
    for i in 0..n {
        g.add_entity(Entity::new(EntityId::new(format!("n{i}")), "Node")).unwrap();
    }
    for i in 0..n - 1 {
        g.add_relationship(Relationship::new(
            EntityId::new(format!("n{i}")),
            EntityId::new(format!("n{}", i + 1)),
            "next",
        ))
        .unwrap();
    }
    g
}

fn bench_entity_lookup(c: &mut Criterion) {
    let g = build_chain(1000);
    c.bench_function("entity_lookup_o1", |b| {
        b.iter(|| {
            let _ = g.get_entity(&EntityId::new("n500"));
        })
    });
}

fn bench_bfs(c: &mut Criterion) {
    let g = build_chain(100);
    c.bench_function("bfs_100_nodes", |b| {
        b.iter(|| {
            let _ = g.bfs(&EntityId::new("n0"), 200);
        })
    });
}

fn bench_shortest_path(c: &mut Criterion) {
    let g = build_chain(50);
    c.bench_function("shortest_path_50_hops", |b| {
        b.iter(|| {
            let _ = g.shortest_path(&EntityId::new("n0"), &EntityId::new("n49"));
        })
    });
}

criterion_group!(benches, bench_entity_lookup, bench_bfs, bench_shortest_path);
criterion_main!(benches);
