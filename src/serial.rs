// SPDX-License-Identifier: MIT
//! Graph serialization: JSON snapshot export and import.
//!
//! ## Responsibility
//! Serialize and deserialize a graph to/from JSON. The snapshot captures
//! entities and relationships and can be used to restore the graph from disk.

use crate::error::GraphError;
use crate::store::GraphStore;
use crate::types::{Entity, Relationship};
use serde::{Deserialize, Serialize};

/// A serializable snapshot of a graph's entities and relationships.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphSnapshot {
    /// All entities in the snapshot.
    pub entities: Vec<Entity>,
    /// All relationships in the snapshot.
    pub relationships: Vec<Relationship>,
}

impl GraphSnapshot {
    /// Create an empty snapshot (for programmatic construction).
    pub fn new() -> Self { Self::default() }

    /// Serialize this snapshot to a JSON string.
    ///
    /// # Returns
    /// - `Ok(String)` — the JSON representation.
    /// - `Err(GraphError::Serialization)` — if serialization fails.
    pub fn to_json(&self) -> Result<String, GraphError> {
        serde_json::to_string(self).map_err(GraphError::Serialization)
    }

    /// Deserialize a snapshot from a JSON string.
    ///
    /// # Returns
    /// - `Ok(GraphSnapshot)` — the deserialized snapshot.
    /// - `Err(GraphError::Serialization)` — if the JSON is invalid.
    pub fn from_json(s: &str) -> Result<Self, GraphError> {
        serde_json::from_str(s).map_err(GraphError::Serialization)
    }

    /// Restore all entities and relationships from this snapshot into `store`.
    ///
    /// Entities are inserted before relationships. Duplicate ids will produce
    /// `GraphError::DuplicateEntity` or `GraphError::DuplicateRelationship`.
    ///
    /// # Returns
    /// - `Ok(())` on full success.
    /// - `Err(GraphError::*)` on the first failure encountered.
    pub fn restore_into(&self, store: &mut GraphStore) -> Result<(), GraphError> {
        for entity in &self.entities {
            store.add_entity(entity.clone())?;
        }
        for rel in &self.relationships {
            store.add_relationship(rel.clone())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::GraphStore;
    use crate::types::{Entity, EntityId, Relationship};

    #[test]
    fn test_snapshot_json_roundtrip_empty() {
        let snap = GraphSnapshot::new();
        let json = snap.to_json().unwrap();
        let decoded = GraphSnapshot::from_json(&json).unwrap();
        assert!(decoded.entities.is_empty());
        assert!(decoded.relationships.is_empty());
    }

    #[test]
    fn test_snapshot_from_json_invalid_returns_serialization_error() {
        let err = GraphSnapshot::from_json("not valid json !!!").unwrap_err();
        assert!(matches!(err, GraphError::Serialization(_)));
    }

    #[test]
    fn test_snapshot_restore_adds_entities() {
        let snap = GraphSnapshot {
            entities: vec![Entity::new(EntityId::new("e1"), "Test")],
            relationships: vec![],
        };
        let mut store = GraphStore::new();
        snap.restore_into(&mut store).unwrap();
        assert_eq!(store.entity_count(), 1);
    }

    #[test]
    fn test_snapshot_restore_adds_relationships() {
        let e1 = Entity::new(EntityId::new("a"), "N");
        let e2 = Entity::new(EntityId::new("b"), "N");
        let rel = Relationship::new(EntityId::new("a"), EntityId::new("b"), "edge");
        let snap = GraphSnapshot { entities: vec![e1, e2], relationships: vec![rel] };
        let mut store = GraphStore::new();
        snap.restore_into(&mut store).unwrap();
        assert_eq!(store.entity_count(), 2);
        assert_eq!(store.edge_count(), 1);
    }

    #[test]
    fn test_snapshot_restore_duplicate_entity_returns_error() {
        let snap = GraphSnapshot {
            entities: vec![
                Entity::new(EntityId::new("dup"), "T"),
                Entity::new(EntityId::new("dup"), "T"),
            ],
            relationships: vec![],
        };
        let mut store = GraphStore::new();
        let err = snap.restore_into(&mut store).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateEntity(_)));
    }

    #[test]
    fn test_snapshot_json_roundtrip_with_entity() {
        let snap = GraphSnapshot {
            entities: vec![Entity::new(EntityId::new("x"), "Kind")],
            relationships: vec![],
        };
        let json = snap.to_json().unwrap();
        let decoded = GraphSnapshot::from_json(&json).unwrap();
        assert_eq!(decoded.entities.len(), 1);
        assert_eq!(decoded.entities[0].id.0, "x");
    }
}
