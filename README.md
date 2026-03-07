# mem-graph

Knowledge graph primitives — entities, typed relationships, properties, and BFS/DFS traversal.

A lightweight in-memory graph store for agent knowledge bases, tool dependency graphs, and semantic memory backends.

## What's inside

- **Entity** — typed nodes with arbitrary property maps
- **Relation** — directed, typed edges between entities
- **Graph** — adjacency-list store with O(1) neighbor lookup
- **BFS / DFS traversal** — depth-limited graph search with visitor pattern
- **Query** — filter by entity type, relation type, property value

## Use cases

- Agent knowledge bases — store facts as (subject, predicate, object) triples
- Tool dependency graphs — model which tools require which capabilities
- Semantic memory backend — back `tokio-agent-memory`'s semantic tier with a real graph
- Reasoning traces — record thought chains as a navigable graph

## Quick start

```rust
use mem_graph::{Graph, Entity, Relation};

let mut g = Graph::new();
let paris = g.add_entity(Entity::new("Paris", "City"));
let france = g.add_entity(Entity::new("France", "Country"));
g.add_relation(paris, france, Relation::new("capital_of"));

let neighbors = g.neighbors(paris);
for (entity, rel) in neighbors {
    println!("{} --[{}]--> {}", "Paris", rel.kind, entity.name);
}
```

## Add to your project

```toml
[dependencies]
mem-graph = { git = "https://github.com/Mattbusel/mem-graph" }
```

## Test coverage

```bash
cargo test
```

---

> Used inside [tokio-prompt-orchestrator](https://github.com/Mattbusel/tokio-prompt-orchestrator) -- a production Rust orchestration layer for LLM pipelines. See the full [primitive library collection](https://github.com/Mattbusel/rust-crates).