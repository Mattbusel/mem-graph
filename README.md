# mem-graph


[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A small in-memory knowledge graph for Rust agents: typed entities and relationships with properties, BFS/DFS, shortest path, transitive closure, time-bounded edges, and JSON snapshots.

Agents that remember facts ("Alice works at Acme", "Acme is in Berlin") need something between a flat vector store and a full graph database. `mem-graph` is a plain Rust struct you can embed in any agent: no server, no async runtime, only `serde`, `uuid`, `chrono` and `thiserror`.

## Features

- **Entities** (`Entity`, `EntityId`) with a kind and typed properties (`PropValue::Text / Number / Bool / Timestamp / List`) plus checked accessors (`as_text`, `as_number`, `as_bool`).
- **Directed, typed relationships** (`Relationship`) with their own properties and an optional validity window (`with_temporal`, `is_valid_at`) for facts that change over time.
- **Integrity checks**: adding an edge to a missing entity, or a duplicate entity or edge, returns a typed `GraphError`.
- **Updates**: `remove_entity` (its relationships go with it), `remove_relationship`, `upsert_entity` (replace properties, keep edges).
- **Queries**: `neighbors_out`, `neighbors_in`, `bfs` and `dfs` with a depth limit, `shortest_path` (BFS, unweighted), `transitive_closure`.
- **Point-in-time queries**: `neighbors_out_at`, `bfs_at` and `shortest_path_at` only follow relationships valid at a given moment, so you can ask where Bob worked last year.
- **Snapshots**: `store.snapshot()` gives a `GraphSnapshot` that serializes to JSON and restores into a `GraphStore`.
- **petgraph** (`petgraph` feature): `to_petgraph` / `from_petgraph` to run any [petgraph](https://crates.io/crates/petgraph) algorithm, and `shortest_path_weighted` (cheapest path by a numeric edge property).

## Install

This crate is not published on crates.io under this name (the `mem-graph` crate there is an unrelated project). Use the git dependency:

```toml
[dependencies]
mem-graph = { git = "https://gitlab.com/mattbusel/mem-graph" }
```

or

```bash
cargo add --git https://gitlab.com/mattbusel/mem-graph mem-graph
```

## Example

```rust
use mem_graph::{Entity, EntityId, GraphError, GraphStore, PropValue, Relationship};

fn main() -> Result<(), GraphError> {
    let mut g = GraphStore::new();

    g.add_entity(Entity::new(EntityId::new("alice"), "Person")
        .with_prop("role", PropValue::Text("engineer".into())))?;
    g.add_entity(Entity::new(EntityId::new("acme"), "Company"))?;
    g.add_entity(Entity::new(EntityId::new("berlin"), "City"))?;

    g.add_relationship(Relationship::new(EntityId::new("alice"), EntityId::new("acme"), "works_at"))?;
    g.add_relationship(Relationship::new(EntityId::new("acme"), EntityId::new("berlin"), "located_in"))?;

    // Who or what does Alice connect to directly?
    for (entity, rel) in g.neighbors_out(&EntityId::new("alice")) {
        println!("alice -{rel}-> {}", entity.id.as_str());
    }

    // Multi-hop reasoning: how is Alice related to Berlin?
    let path = g.shortest_path(&EntityId::new("alice"), &EntityId::new("berlin"))?;
    println!("{:?}", path); // Some([EntityId("alice"), EntityId("acme"), EntityId("berlin")])

    // Everything reachable from Alice, up to 2 hops.
    let reachable = g.bfs(&EntityId::new("alice"), 2)?;
    assert_eq!(reachable.len(), 3);

    let role = g.get_entity(&EntityId::new("alice"))?.get_prop("role");
    println!("role: {:?}", role);
    Ok(())
}
```

## Facts that change over time

```rust
use chrono::{Duration, Utc};
use mem_graph::{Entity, EntityId, GraphStore, Relationship};

let mut g = GraphStore::new();
for (id, kind) in [("bob", "Person"), ("globex", "Company"), ("acme", "Company")] {
    g.add_entity(Entity::new(EntityId::new(id), kind)).unwrap();
}
let now = Utc::now();
let switched = now - Duration::days(30);
g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("globex"), "works_at")
    .with_temporal(now - Duration::days(1000), Some(switched))).unwrap();
g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("acme"), "works_at")
    .with_temporal(switched, None)).unwrap();

let employer_at = |t| g.neighbors_out_at(&EntityId::new("bob"), t)[0].0.id.0.clone();
assert_eq!(employer_at(now - Duration::days(365)), "globex");
assert_eq!(employer_at(now), "acme");
```

## Weighted paths and petgraph algorithms

```toml
mem-graph = { git = "https://gitlab.com/mattbusel/mem-graph", features = ["petgraph"] }
```

```rust,ignore
// Cheapest route by the "km" property on each relationship (1.0 when missing).
let (km, path) = g.shortest_path_weighted(&EntityId::new("a"), &EntityId::new("d"), "km")?.unwrap();

// Or hand the graph to petgraph: cycles, topological order, PageRank and more.
let (pg, nodes) = g.to_petgraph();
let cycles = petgraph::algo::tarjan_scc(&pg);
```

## How it works

| File | What it holds |
|---|---|
| `src/types.rs` | `EntityId`, `Entity`, `Relationship`, `PropValue` |
| `src/store.rs` | `GraphStore`: entity map, edge map keyed by `(from, to, rel_type)`, and in/out adjacency lists for O(degree) neighbor lookup; all traversals |
| `src/serial.rs` | `GraphSnapshot` JSON round-trip and `restore_into` |
| `src/error.rs` | `GraphError` |

## Status and limitations

Version 0.2, in-memory only (persist with `snapshot().to_json()`).

- `bfs`, `dfs` and `shortest_path` see every relationship; use the `_at` variants to respect validity windows.
- Requires Rust 1.82 or newer (`Option::is_none_or`).

```bash
cargo test --all-features
cargo bench
```

## License

MIT, see [LICENSE](LICENSE).

---

Part of a set of Rust crates for LLM agents, see [rust-crates](https://gitlab.com/mattbusel/rust-crates).
