// SPDX-License-Identifier: MIT
//! Graph serialization: snapshots, JSON and binary save/load.
//!
//! ## Responsibility
//! Serialize and deserialize a graph. A [`GraphSnapshot`] captures every
//! entity and relationship in a deterministic order and restores atomically.
//!
//! ## Formats
//! - **serde.** [`GraphStore`], [`GraphSnapshot`], [`Entity`] and
//!   [`Relationship`] implement `Serialize` and `Deserialize`, so a graph can
//!   be embedded in your own serde types and written with any serde format.
//!   A `GraphStore` serializes as its snapshot; deserializing validates it
//!   (duplicate ids, dangling endpoints) and fails with a serde error
//!   otherwise.
//! - **JSON** (always available): [`GraphStore::to_json`] /
//!   [`GraphStore::from_json`], [`GraphStore::save_json`] /
//!   [`GraphStore::load_json`] for files, and `write_json` / `read_json` for
//!   any `std::io` writer or reader.
//! - **Binary** (the `binary` cargo feature): a compact
//!   [postcard](https://docs.rs/postcard) encoding behind a short header
//!   (`MGRAPH` plus a format version byte), with `to_bytes` / `from_bytes`
//!   and `save_binary` / `load_binary`. It is several times smaller than
//!   JSON and faster to parse, but not human-readable.
//!
//! File saves are atomic: the data goes to a temporary file next to the
//! target, which is then renamed over it, so a crash mid-save leaves the old
//! file intact.
//!
//! ## Example
//! ```rust
//! use mem_graph::{Entity, EntityId, GraphStore, Relationship};
//!
//! let mut g = GraphStore::new();
//! g.add_entity(Entity::new(EntityId::new("alice"), "Person")).unwrap();
//! g.add_entity(Entity::new(EntityId::new("acme"), "Company")).unwrap();
//! g.add_relationship(Relationship::new(EntityId::new("alice"), EntityId::new("acme"), "works_at")).unwrap();
//!
//! let path = std::env::temp_dir().join(format!("mem-graph-doc-{}.json", std::process::id()));
//! g.save_json(&path).unwrap();
//! let back = GraphStore::load_json(&path).unwrap();
//! assert_eq!(back.to_json().unwrap(), g.to_json().unwrap());
//! # std::fs::remove_file(&path).unwrap();
//! ```

use crate::error::GraphError;
use crate::store::GraphStore;
use crate::types::{Entity, Relationship};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;

/// A serializable snapshot of a graph's entities and relationships.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
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

    /// Serialize this snapshot to indented, human-readable JSON.
    ///
    /// # Errors
    /// [`GraphError::Serialization`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, GraphError> {
        serde_json::to_string_pretty(self).map_err(GraphError::Serialization)
    }

    /// Write this snapshot as compact JSON to `writer`. Pass a buffered
    /// writer (`BufWriter`) for files and sockets.
    ///
    /// # Errors
    /// [`GraphError::Io`] if writing fails, [`GraphError::Serialization`]
    /// otherwise.
    pub fn write_json<W: Write>(&self, writer: W) -> Result<(), GraphError> {
        serde_json::to_writer(writer, self).map_err(json_error)
    }

    /// Read a snapshot from JSON produced by [`write_json`](Self::write_json),
    /// [`to_json`](Self::to_json) or [`to_json_pretty`](Self::to_json_pretty).
    ///
    /// # Errors
    /// [`GraphError::Io`] if reading fails, [`GraphError::Serialization`] if
    /// the JSON is invalid.
    pub fn read_json<R: Read>(mut reader: R) -> Result<Self, GraphError> {
        // Reading into memory first is much faster than serde_json::from_reader.
        let mut text = String::new();
        reader.read_to_string(&mut text)?;
        Self::from_json(&text)
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

    /// Encode this snapshot in the compact binary format (see the
    /// [module docs](self)).
    ///
    /// # Errors
    /// [`GraphError::Binary`] if encoding fails.
    #[cfg(feature = "binary")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, GraphError> {
        let mut out = Vec::with_capacity(BINARY_MAGIC.len() + 1 + 64);
        out.extend_from_slice(BINARY_MAGIC);
        out.push(BINARY_VERSION);
        postcard::to_io(self, &mut out).map_err(|e| GraphError::Binary(e.to_string()))?;
        Ok(out)
    }

    /// Decode a snapshot written by [`to_bytes`](Self::to_bytes).
    ///
    /// # Errors
    /// [`GraphError::Binary`] if the bytes do not start with the mem-graph
    /// header, were written by an unsupported format version, are truncated,
    /// corrupt, or have trailing bytes.
    #[cfg(feature = "binary")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GraphError> {
        let body = bytes
            .strip_prefix(BINARY_MAGIC)
            .ok_or_else(|| GraphError::Binary("not a mem-graph binary snapshot (bad header)".into()))?;
        let (&version, body) = body
            .split_first()
            .ok_or_else(|| GraphError::Binary("truncated header: missing format version".into()))?;
        if version != BINARY_VERSION {
            return Err(GraphError::Binary(format!(
                "unsupported format version {version} (this build reads version {BINARY_VERSION})"
            )));
        }
        let (snapshot, rest) =
            postcard::take_from_bytes::<Self>(body).map_err(|e| GraphError::Binary(e.to_string()))?;
        if !rest.is_empty() {
            return Err(GraphError::Binary(format!("{} unexpected trailing bytes", rest.len())));
        }
        Ok(snapshot)
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

/// First bytes of every binary snapshot.
#[cfg(feature = "binary")]
const BINARY_MAGIC: &[u8] = b"MGRAPH";
/// Binary format version written after the magic.
#[cfg(feature = "binary")]
const BINARY_VERSION: u8 = 1;

/// Map a serde_json error to `Io` when it came from the reader/writer.
fn json_error(e: serde_json::Error) -> GraphError {
    if e.is_io() {
        GraphError::Io(e.into())
    } else {
        GraphError::Serialization(e)
    }
}

/// Run `write` against a temporary file and move it to `path` atomically: write a temporary file in the same
/// directory, flush it to disk, then rename it over `path`.
fn write_atomic(path: &Path, write: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> Result<(), GraphError>) -> Result<(), GraphError> {
    let name = path
        .file_name()
        .ok_or_else(|| GraphError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    let result = (|| {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        write(&mut out)?;
        let file = out.into_inner().map_err(|e| GraphError::Io(e.into_error()))?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// A `GraphStore` serializes as its [`GraphSnapshot`] (deterministic order).
impl Serialize for GraphStore {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.snapshot().serialize(serializer)
    }
}

/// A `GraphStore` deserializes from a [`GraphSnapshot`]; a snapshot with
/// duplicate ids or relationships, or a relationship whose endpoint is
/// missing, is rejected with a serde error.
impl<'de> Deserialize<'de> for GraphStore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let snapshot = GraphSnapshot::deserialize(deserializer)?;
        GraphStore::from_snapshot(&snapshot).map_err(serde::de::Error::custom)
    }
}

impl GraphStore {
    /// The whole graph as compact JSON (the [`snapshot`](Self::snapshot)
    /// format, deterministic for the same graph).
    ///
    /// # Errors
    /// [`GraphError::Serialization`] if serialization fails.
    pub fn to_json(&self) -> Result<String, GraphError> {
        self.snapshot().to_json()
    }

    /// The whole graph as indented, human-readable JSON.
    ///
    /// # Errors
    /// [`GraphError::Serialization`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, GraphError> {
        self.snapshot().to_json_pretty()
    }

    /// Build a store from JSON written by [`to_json`](Self::to_json),
    /// [`to_json_pretty`](Self::to_json_pretty) or
    /// [`GraphSnapshot::to_json`].
    ///
    /// # Errors
    /// [`GraphError::Serialization`] for invalid JSON, and the
    /// [`from_snapshot`](Self::from_snapshot) errors for an inconsistent
    /// graph.
    pub fn from_json(json: &str) -> Result<Self, GraphError> {
        Self::from_snapshot(&GraphSnapshot::from_json(json)?)
    }

    /// Write the whole graph as compact JSON to `writer`. Pass a buffered
    /// writer (`BufWriter`) for files and sockets.
    ///
    /// # Errors
    /// [`GraphError::Io`] if writing fails.
    pub fn write_json<W: Write>(&self, writer: W) -> Result<(), GraphError> {
        self.snapshot().write_json(writer)
    }

    /// Read a graph from JSON (see [`from_json`](Self::from_json)).
    ///
    /// # Errors
    /// [`GraphError::Io`] if reading fails, otherwise as
    /// [`from_json`](Self::from_json).
    pub fn read_json<R: Read>(reader: R) -> Result<Self, GraphError> {
        Self::from_snapshot(&GraphSnapshot::read_json(reader)?)
    }

    /// Save the whole graph to a JSON file, atomically (a temporary file in
    /// the same directory is renamed over `path`).
    ///
    /// # Errors
    /// [`GraphError::Io`] if the file cannot be written.
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), GraphError> {
        let snapshot = self.snapshot();
        write_atomic(path.as_ref(), |out| snapshot.write_json(out))
    }

    /// Load a graph saved with [`save_json`](Self::save_json).
    ///
    /// # Errors
    /// [`GraphError::Io`] if the file cannot be read, otherwise as
    /// [`from_json`](Self::from_json).
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, GraphError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// The whole graph in the compact binary format (see the
    /// [module docs](crate::serial)).
    ///
    /// # Example
    /// ```rust
    /// use mem_graph::{Entity, EntityId, GraphStore};
    ///
    /// let mut g = GraphStore::new();
    /// g.add_entity(Entity::new(EntityId::new("alice"), "Person")).unwrap();
    /// let bytes = g.to_bytes().unwrap();
    /// assert!(bytes.len() < g.to_json().unwrap().len());
    /// let back = GraphStore::from_bytes(&bytes).unwrap();
    /// assert_eq!(back.entity_count(), 1);
    /// ```
    ///
    /// # Errors
    /// [`GraphError::Binary`] if encoding fails.
    #[cfg(feature = "binary")]
    pub fn to_bytes(&self) -> Result<Vec<u8>, GraphError> {
        self.snapshot().to_bytes()
    }

    /// Build a store from bytes written by [`to_bytes`](Self::to_bytes).
    ///
    /// # Errors
    /// [`GraphError::Binary`] for bytes that are not a valid binary
    /// snapshot, and the [`from_snapshot`](Self::from_snapshot) errors for an
    /// inconsistent graph.
    #[cfg(feature = "binary")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GraphError> {
        Self::from_snapshot(&GraphSnapshot::from_bytes(bytes)?)
    }

    /// Save the whole graph to a binary file, atomically.
    ///
    /// # Errors
    /// [`GraphError::Io`] if the file cannot be written, [`GraphError::Binary`]
    /// if encoding fails.
    #[cfg(feature = "binary")]
    pub fn save_binary(&self, path: impl AsRef<Path>) -> Result<(), GraphError> {
        let bytes = self.to_bytes()?;
        write_atomic(path.as_ref(), |out| Ok(out.write_all(&bytes)?))
    }

    /// Load a graph saved with [`save_binary`](Self::save_binary).
    ///
    /// # Errors
    /// [`GraphError::Io`] if the file cannot be read, otherwise as
    /// [`from_bytes`](Self::from_bytes).
    #[cfg(feature = "binary")]
    pub fn load_binary(path: impl AsRef<Path>) -> Result<Self, GraphError> {
        Self::from_bytes(&std::fs::read(path)?)
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
