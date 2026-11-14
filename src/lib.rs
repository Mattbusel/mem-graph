// SPDX-License-Identifier: MIT
//! # mem-graph
//!
//! An embeddable, in-memory knowledge graph for agents: typed entities and
//! relationships with property maps, BFS/DFS traversal, shortest path,
//! transitive closure, time-bounded facts, and JSON snapshots.
//!
//! Published on crates.io as `mattbusel-mem-graph`; the library name is
//! `mem_graph`, so code imports it as `use mem_graph::...`.
//!
//! ## What is in the box
//!
//! - [`GraphStore`]: the whole engine. O(1) entity lookup, O(degree)
//!   [`neighbors_out`](GraphStore::neighbors_out) /
//!   [`neighbors_in`](GraphStore::neighbors_in), depth-limited
//!   [`bfs`](GraphStore::bfs) and [`dfs`](GraphStore::dfs) (iterative, so
//!   deep graphs do not overflow the stack),
//!   [`shortest_path`](GraphStore::shortest_path) and
//!   [`transitive_closure`](GraphStore::transitive_closure). It is `Clone`,
//!   `Debug` and `Default`.
//! - **Point-in-time queries.** A [`Relationship`] can carry a validity window
//!   ([`Relationship::with_temporal`]). The `_at` queries only follow
//!   relationships valid at a given instant, so you can ask what the graph
//!   looked like then: [`neighbors_out_at`](GraphStore::neighbors_out_at),
//!   [`bfs_at`](GraphStore::bfs_at) and
//!   [`shortest_path_at`](GraphStore::shortest_path_at). The plain queries see
//!   every relationship regardless of its window.
//! - **Snapshots.** [`GraphStore::snapshot`] returns a
//!   [`serial::GraphSnapshot`] with a deterministic order (entities by id,
//!   relationships by `(from, to, rel_type)`, property keys sorted), which
//!   serializes to JSON and restores atomically with
//!   [`restore_into`](serial::GraphSnapshot::restore_into) or
//!   [`GraphStore::from_snapshot`].
//! - **petgraph interop** (the `petgraph` cargo feature): `to_petgraph`,
//!   `from_petgraph` and `shortest_path_weighted` (cheapest path by a numeric
//!   relationship property), so any petgraph algorithm (strongly connected
//!   components, topological sort, PageRank, ...) can run on the graph.
//!
//! The crate contains no `unsafe` code (`#![forbid(unsafe_code)]`).
//!
//! ## Example
//! ```rust
//! use mem_graph::{GraphStore, Entity, EntityId, Relationship};
//!
//! let mut store = GraphStore::new();
//! store.add_entity(Entity::new(EntityId::new("alice"), "Person")).unwrap();
//! store.add_entity(Entity::new(EntityId::new("bob"), "Person")).unwrap();
//! store.add_relationship(
//!     Relationship::new(EntityId::new("alice"), EntityId::new("bob"), "knows")
//! ).unwrap();
//!
//! let path = store.shortest_path(&EntityId::new("alice"), &EntityId::new("bob"))
//!     .unwrap()
//!     .unwrap();
//! assert_eq!(path.len(), 2);
//! ```
//!
//! ## Point-in-time example
//! ```rust
//! use chrono::{Duration, Utc};
//! use mem_graph::{Entity, EntityId, GraphStore, Relationship};
//!
//! let mut g = GraphStore::new();
//! for id in ["bob", "globex", "acme"] {
//!     g.add_entity(Entity::new(EntityId::new(id), "Node")).unwrap();
//! }
//! let now = Utc::now();
//! let moved = now - Duration::days(30);
//! g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("globex"), "works_at")
//!     .with_temporal(now - Duration::days(900), Some(moved))).unwrap();
//! g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("acme"), "works_at")
//!     .with_temporal(moved, None)).unwrap();
//!
//! let a_year_ago = g.bfs_at(&EntityId::new("bob"), 1, now - Duration::days(365)).unwrap();
//! assert_eq!(a_year_ago, [EntityId::new("bob"), EntityId::new("globex")]);
//! let today = g.bfs_at(&EntityId::new("bob"), 1, now).unwrap();
//! assert_eq!(today, [EntityId::new("bob"), EntityId::new("acme")]);
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod error;
pub mod serial;
pub mod store;
pub mod types;

pub use error::GraphError;
pub use store::GraphStore;
pub use types::{Entity, EntityId, PropValue, Relationship};

/// Every Rust example in the README is compiled by `cargo test --doc`.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
mod readme_examples {}
