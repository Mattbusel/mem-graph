# Changelog

## [0.2.0] - 2026-10-04

### Added

- `remove_entity` (removes the entity's relationships from every index),
  `remove_relationship`, `upsert_entity`, `entities()`, `relationships()`.
- `GraphStore::snapshot()`: the whole graph as a `GraphSnapshot`.
- Point-in-time queries that respect relationship validity windows:
  `neighbors_out_at`, `bfs_at`, `shortest_path_at`.
- `petgraph` feature: `to_petgraph` / `from_petgraph`, and `shortest_path_weighted`
  (cheapest path by a numeric edge property; negative or non-numeric weights are an error).
- Tests for all of the above; README examples are doctests.

### Changed

- `rust-version = "1.82"` declared (it was only mentioned in the README) and checked
  with an MSRV-aware lockfile; crates.io metadata and GitLab CI added.
