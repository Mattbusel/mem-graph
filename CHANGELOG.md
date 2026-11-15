# Changelog

## [0.3.0] - Unreleased

Everything is additive; code written against 0.2 compiles unchanged.

```toml
[dependencies]
mattbusel-mem-graph = { version = "0.3", features = ["binary", "petgraph"] }
```

### Added

- **Query helpers** (new `query` module, all methods on `GraphStore`):
  - Neighbors by relationship type: `neighbors_out_by_type`,
    `neighbors_in_by_type`, and `neighbors_by_type` with a `Direction`
    (`Outgoing`, `Incoming` or `Both`; re-exported as `mem_graph::Direction`).
  - `relationships_of(id, direction)`, `relationships_of_type(rel_type)` and
    `entities_of_kind(kind)`.
  - Filtered shortest paths: `shortest_path_via(from, to, &["reports_to"])`
    follows only the listed relationship types, and
    `shortest_path_where(from, to, direction, predicate)` follows any
    relationships a closure accepts, in either direction (so undirected
    paths are possible).
  - Subgraph extraction: `within(start, depth, direction)` lists the ids in
    range, `subgraph(start, depth, direction)` returns them as a new
    independent `GraphStore` with every relationship between them (the
    induced subgraph), and `induced_subgraph(ids)` does the same for any list
    of ids.
  - Text search: `search_entities(query)` is a case-insensitive substring
    match (Unicode-aware) over ids, kinds and text properties, including text
    inside list properties; `search_entities_by_prop(key, query)` restricts it
    to one property; `find_by_prop(key, value)` is an exact match. Results are
    sorted by id.
- **serde and save/load.** `GraphStore` now implements `Serialize` and
  `Deserialize` (as its snapshot; deserializing validates the graph and
  rejects duplicate ids or dangling relationships with a serde error), so it
  can be a field in your own serde types. `Entity`, `Relationship` and
  `GraphSnapshot` already did. New JSON helpers: `GraphStore::to_json`,
  `to_json_pretty`, `from_json`, `write_json` / `read_json` (any `std::io`
  writer or reader), and `save_json` / `load_json` for files.
  `GraphSnapshot` gains `to_json_pretty`, `write_json` and `read_json`.
- **Compact binary format** (new `binary` feature, uses postcard):
  `to_bytes` / `from_bytes` on `GraphStore` and `GraphSnapshot`, and
  `save_binary` / `load_binary`. The bytes start with a `MGRAPH` header and a
  format version byte, so foreign, truncated, padded or future-version data
  is rejected with `GraphError::Binary` instead of being misread. postcard was
  picked over bincode because bincode is no longer maintained.
- File saves (`save_json`, `save_binary`) are atomic: they write a temporary
  file next to the target and rename it over the target, so a crash mid-save
  keeps the previous file.
- **petgraph** feature: `to_petgraph_graph` / `from_petgraph_graph` convert to
  and from a plain `petgraph::Graph` (`DiGraph<Entity, Relationship>`), next
  to the existing `StableDiGraph` ones. `from_petgraph_with(graph, node_fn,
  edge_fn)` imports any directed `petgraph::Graph`, whatever its node and
  edge weights, through two closures. New `GraphNodeMap` type alias.
- `GraphError::Io` (file and reader/writer failures) and `GraphError::Binary`.
  `GraphError` is `#[non_exhaustive]`, so this is not breaking.
- `Entity`, `Relationship` and `GraphSnapshot` derive `PartialEq`.
- `GraphSnapshot` is re-exported at the crate root.
- A `serde` feature that does nothing: serde support is always compiled in,
  and the feature exists only so `features = ["serde"]` resolves.
- `examples/agent_memory.rs`: long-term memory for an LLM agent. It stores
  facts as triples, ends a fact when a newer one replaces it (so past states
  stay queryable), recalls prompt-ready sentences with text search plus a
  one-hop subgraph, and persists across sessions with `save_json` /
  `load_json`. Its docs show how to expose `remember` and `recall` as rig
  tools. Run it with `cargo run --example agent_memory`.

### Changed

- `to_petgraph` now adds nodes in entity id order and edges in
  `(from, to, rel_type)` order, so the same graph always gets the same
  petgraph indices (it used `HashMap` order before).

## [0.2.0] - 2026-11-15

First release of the 0.2 line on crates.io. It is published as
`mattbusel-mem-graph` (the crates.io name `mem-graph` belongs to an unrelated
crate), and the library is named `mem_graph`, so code imports it as
`use mem_graph::...`:

```toml
[dependencies]
mattbusel-mem-graph = "0.2"
```

0.1.x was published with the library name `mattbusel_mem_graph`; when upgrading,
change `use mattbusel_mem_graph::...` to `use mem_graph::...`.

### Fixed

- `dfs` with a depth limit missed nodes that were within the limit. A node first
  reached through a long path at exactly `max_depth` was marked visited and never
  expanded, so when a shorter path reached it later, its children were skipped
  (edges `a->b, a->x, b->x, x->y`: `dfs(a, 2)` returned `[a, b, x]` without `y`).
  `dfs` now tracks the smallest depth each node was reached at and expands a node
  again when a shorter path reaches it. Each node is still reported once, in the
  same depth-first preorder. Reported by mattabate in
  [#1](https://github.com/Mattbusel/mem-graph/issues/1). Thank you.
- `dfs` was recursive and overflowed the stack on deep graphs (a 200 000-node
  chain aborted the process). It is now iterative.
- `GraphSnapshot::restore_into` is atomic: the snapshot is validated against
  itself and the target store (duplicate ids, duplicate relationships, dangling
  endpoints) before anything is inserted. Previously an error part-way through
  left the store half-populated.

### Added

- `remove_entity` (removes the entity's relationships from every index),
  `remove_relationship`, `upsert_entity`, `entities()`, `relationships()`.
- `GraphStore::snapshot()`: the whole graph as a `GraphSnapshot`, in a
  deterministic order (entities by id, relationships by `(from, to, rel_type)`,
  property keys sorted in JSON), so two snapshots of the same graph produce
  byte-identical JSON.
- `GraphStore::from_snapshot`.
- `GraphStore` derives `Debug`, `Clone` and `Default`, so it can sit inside a
  struct that derives them.
- `impl Borrow<str> for EntityId`: a `HashSet<EntityId>` can be queried with
  `contains("alice")`.
- Point-in-time queries that respect relationship validity windows:
  `neighbors_out_at`, `bfs_at`, `shortest_path_at`.
- `petgraph` feature: `to_petgraph` / `from_petgraph`, and `shortest_path_weighted`
  (cheapest path by a numeric edge property; negative or non-numeric weights are an error).
- Property tests (`tests/traversal_props.rs`): on random graphs, `dfs` and `bfs`
  reach the same set, traversals agree with `shortest_path` and
  `transitive_closure`, and the `_at` queries match the plain ones when no
  validity windows are set. README examples are doctests.
- Crate docs cover the point-in-time queries, snapshots and petgraph interop;
  docs.rs marks the `petgraph`-only items. `#![forbid(unsafe_code)]`.

### Changed (breaking)

- Package renamed to `mattbusel-mem-graph`, library name `mem_graph` (see above).
- `GraphError` is `#[non_exhaustive]`; the never-constructed `CycleDetected`
  variant is removed.
- `transitive_closure` returns `HashSet<EntityId>` instead of `HashSet<String>`,
  like every other query. `closure.contains("id")` still compiles.

### Changed

- `rust-version = "1.82"` declared (it was only mentioned in the README) and checked
  with an MSRV-aware lockfile; crates.io metadata and GitLab CI added.
