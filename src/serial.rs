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
use std::collections::HashSet;

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
    /// The restore is atomic: the whole snapshot is checked against itself
    /// and against `store` first, and nothing is inserted unless every item
    /// can be. On error, `store` is left exactly as it was.
    ///
    /// # Errors
    /// - [`GraphError::DuplicateEntity`] if an entity id appears twice in the
    ///   snapshot or already exists in `store`.
    /// - [`GraphError::EntityNotFound`] if a relationship's endpoint is neither
    ///   in the snapshot nor in `store`.
    /// - [`GraphError::DuplicateRelationship`] if a `(from, to, rel_type)`
    ///   triple appears twice in the snapshot or already exists in `store`.
    ///
    /// Entities are checked before relationships, in snapshot order, and the
    /// first problem found is returned.
    pub fn restore_into(&self, store: &mut GraphStore) -> Result<(), GraphError> {
        self.validate_against(store)?;
        // Validation guarantees every insert below succeeds.
        for entity in &self.entities {
            store.add_entity(entity.clone())?;
        }
        for rel in &self.relationships {
            store.add_relationship(rel.clone())?;
        }
        Ok(())
    }

    /// Check that every item of this snapshot can be inserted into `store`.
    fn validate_against(&self, store: &GraphStore) -> Result<(), GraphError> {
        let mut new_ids: HashSet<&str> = HashSet::with_capacity(self.entities.len());
        for entity in &self.entities {
            let id = entity.id.as_str();
            if store.get_entity(&entity.id).is_ok() || !new_ids.insert(id) {
                return Err(GraphError::DuplicateEntity(id.to_string()));
            }
        }
        let mut new_edges: HashSet<(&str, &str, &str)> =
            HashSet::with_capacity(self.relationships.len());
        for rel in &self.relationships {
            for end in [&rel.from, &rel.to] {
                if !new_ids.contains(end.as_str()) && store.get_entity(end).is_err() {
                    return Err(GraphError::EntityNotFound(end.0.clone()));
                }
            }
            let triple = (rel.from.as_str(), rel.to.as_str(), rel.rel_type.as_str());
            let exists = store.get_relationship(&rel.from, &rel.to, &rel.rel_type).is_ok();
            if exists || !new_edges.insert(triple) {
                return Err(GraphError::DuplicateRelationship {
                    from: rel.from.0.clone(),
                    to: rel.to.0.clone(),
                    rel: rel.rel_type.clone(),
                });
            }
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

    #[test]
    fn test_snapshot_restore_is_atomic_on_duplicate_entity() {
        let mut store = GraphStore::new();
        store.add_entity(Entity::new(EntityId::new("taken"), "T")).unwrap();
        let snap = GraphSnapshot {
            entities: vec![
                Entity::new(EntityId::new("fresh"), "T"),
                Entity::new(EntityId::new("taken"), "T"),
            ],
            relationships: vec![],
        };
        let err = snap.restore_into(&mut store).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateEntity(ref id) if id == "taken"));
        assert_eq!(store.entity_count(), 1, "nothing may be inserted on error");
        assert!(store.get_entity(&EntityId::new("fresh")).is_err());
    }

    #[test]
    fn test_snapshot_restore_is_atomic_on_dangling_relationship() {
        let snap = GraphSnapshot {
            entities: vec![Entity::new(EntityId::new("a"), "N")],
            relationships: vec![Relationship::new(EntityId::new("a"), EntityId::new("ghost"), "r")],
        };
        let mut store = GraphStore::new();
        let err = snap.restore_into(&mut store).unwrap_err();
        assert!(matches!(err, GraphError::EntityNotFound(ref id) if id == "ghost"));
        assert_eq!(store.entity_count(), 0);
        assert_eq!(store.edge_count(), 0);
    }

    #[test]
    fn test_snapshot_restore_is_atomic_on_duplicate_relationship() {
        let snap = GraphSnapshot {
            entities: vec![Entity::new(EntityId::new("a"), "N"), Entity::new(EntityId::new("b"), "N")],
            relationships: vec![
                Relationship::new(EntityId::new("a"), EntityId::new("b"), "r"),
                Relationship::new(EntityId::new("a"), EntityId::new("b"), "r"),
            ],
        };
        let mut store = GraphStore::new();
        let err = snap.restore_into(&mut store).unwrap_err();
        assert!(matches!(err, GraphError::DuplicateRelationship { .. }));
        assert_eq!(store.entity_count(), 0);
    }

    #[test]
    fn test_snapshot_restore_can_link_to_entities_already_in_store() {
        let mut store = GraphStore::new();
        store.add_entity(Entity::new(EntityId::new("old"), "N")).unwrap();
        let snap = GraphSnapshot {
            entities: vec![Entity::new(EntityId::new("new"), "N")],
            relationships: vec![Relationship::new(EntityId::new("new"), EntityId::new("old"), "r")],
        };
        snap.restore_into(&mut store).unwrap();
        assert_eq!(store.edge_count(), 1);
    }

    #[test]
    fn test_snapshot_is_sorted_and_json_is_deterministic() {
        use crate::types::PropValue;
        let mut g = GraphStore::new();
        for id in ["c", "a", "b"] {
            let mut e = Entity::new(EntityId::new(id), "N");
            for k in ["k_z", "k_m", "k_a", "k_q"] {
                e = e.with_prop(k, PropValue::Bool(true));
            }
            g.add_entity(e).unwrap();
        }
        for (f, t, r) in [("c", "a", "y"), ("a", "b", "x"), ("a", "b", "w"), ("b", "c", "v")] {
            g.add_relationship(Relationship::new(EntityId::new(f), EntityId::new(t), r)).unwrap();
        }
        let snap = g.snapshot();
        let ids: Vec<&str> = snap.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c"]);
        let edges: Vec<(&str, &str, &str)> = snap
            .relationships
            .iter()
            .map(|r| (r.from.as_str(), r.to.as_str(), r.rel_type.as_str()))
            .collect();
        assert_eq!(edges, [("a", "b", "w"), ("a", "b", "x"), ("b", "c", "v"), ("c", "a", "y")]);

        // Property keys serialize sorted (the map is a HashMap in memory).
        let json = snap.to_json().unwrap();
        let pos = |k: &str| json.find(&format!("\"{k}\":")).unwrap();
        assert!(pos("k_a") < pos("k_m") && pos("k_m") < pos("k_q") && pos("k_q") < pos("k_z"));
        // The same items inserted in another order (into maps with a fresh
        // random hasher) snapshot to byte-identical JSON.
        let mut reordered = GraphStore::new();
        for e in snap.entities.iter().rev() {
            reordered.add_entity(e.clone()).unwrap();
        }
        for r in snap.relationships.iter().rev() {
            reordered.add_relationship(r.clone()).unwrap();
        }
        assert_eq!(reordered.snapshot().to_json().unwrap(), json);
    }
}
