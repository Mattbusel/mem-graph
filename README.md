# mem-graph

[![CI](https://github.com/Mattbusel/mem-graph/actions/workflows/ci.yml/badge.svg)](https://github.com/Mattbusel/mem-graph/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A small in-memory knowledge graph for Rust agents: typed entities and relationships with properties, BFS/DFS, shortest path, transitive closure, time-bounded edges, and JSON snapshots.

Agents that remember facts ("Alice works at Acme", "Acme is in Berlin") need something between a flat vector store and a full graph database. `mem-graph` is a plain Rust struct you can embed in any agent: no server, no async runtime, only `serde`, `uuid`, `chrono` and `thiserror`.

## Features

- **Entities** (`Entity`, `EntityId`) with a kind and typed properties (`PropValue::Text / Number / Bool / Timestamp / List`) plus checked accessors (`as_text`, `as_number`, `as_bool`).
- **Directed, typed relationships** (`Relationship`) with their own properties and an optional validity window (`with_temporal`, `is_valid_at`) for facts that change over time.
- **Integrity checks**: adding an edge to a missing entity, or a duplicate entity or edge, returns a typed `GraphError`.
- **Queries**: `neighbors_out`, `neighbors_in`, `bfs` and `dfs` with a depth limit, `shortest_path` (BFS, unweighted), `transitive_closure`.
- **Snapshots**: `GraphSnapshot` serializes entities and relationships to JSON and restores them into a `GraphStore`.

## Install

This crate is not published on crates.io under this name (the `mem-graph` crate there is an unrelated project). Use the git dependency:

```toml
[dependencies]
mem-graph = { git = "https://gitlab.com/mattbusel/mem-graph" }
```

or

```bash
cargo add --git https://github.com/Mattbusel/mem-graph mem-graph
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

## How it works

| File | What it holds |
|---|---|
| `src/types.rs` | `EntityId`, `Entity`, `Relationship`, `PropValue` |
| `src/store.rs` | `GraphStore`: entity map, edge map keyed by `(from, to, rel_type)`, and in/out adjacency lists for O(degree) neighbor lookup; all traversals |
| `src/serial.rs` | `GraphSnapshot` JSON round-trip and `restore_into` |
| `src/error.rs` | `GraphError` |

## Status and limitations

Version 0.1, in-memory only.

- There are no delete operations yet.
- `GraphSnapshot` is filled by hand (its `entities` and `relationships` fields are public); there is no `GraphStore::snapshot()` helper.
- Traversals ignore the temporal window; filter with `Relationship::is_valid_at` yourself.
- Requires Rust 1.82 or newer (`Option::is_none_or`).

```bash
cargo test
cargo bench
```

## License

MIT, see [LICENSE](LICENSE).

---

Part of a set of Rust crates for LLM agents, see [rust-crates](https://github.com/Mattbusel/rust-crates).
