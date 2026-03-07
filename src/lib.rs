// SPDX-License-Identifier: MIT
//! # mem-graph
//!
//! Knowledge graph primitives: entities, typed relationships, BFS/DFS traversal,
//! shortest-path, transitive closure, and JSON snapshot serialization.
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

pub mod error;
pub mod serial;
pub mod store;
pub mod types;

pub use error::GraphError;
pub use store::GraphStore;
pub use types::{Entity, EntityId, PropValue, Relationship};
