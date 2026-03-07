// SPDX-License-Identifier: MIT
use thiserror::Error;

/// All errors that can occur in the mem-graph crate.
#[derive(Debug, Error)]
pub enum GraphError {
    /// Entity with the given id was not found in the store.
    #[error("Entity '{0}' not found")]
    EntityNotFound(String),

    /// A relationship between two entities with the given type was not found.
    #[error("Relationship '{rel}' from '{from}' to '{to}' not found")]
    RelationshipNotFound { from: String, to: String, rel: String },

    /// An entity with this id already exists.
    #[error("Duplicate entity id: '{0}'")]
    DuplicateEntity(String),

    /// A relationship with this exact triple already exists.
    #[error("Duplicate relationship: '{rel}' from '{from}' to '{to}' already exists")]
    DuplicateRelationship { from: String, to: String, rel: String },

    /// A property was accessed with the wrong type.
    #[error("Property type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },

    /// A cycle was detected during traversal.
    #[error("Cycle detected in graph traversal starting from '{0}'")]
    CycleDetected(String),

    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A property value was invalid.
    #[error("Invalid property value: {0}")]
    InvalidProperty(String),
}
