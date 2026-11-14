# Changelog

## [0.2.0] - Unreleased

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
