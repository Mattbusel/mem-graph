// SPDX-License-Identifier: MIT
use thiserror::Error;

/// All errors that can occur in the mem-graph crate.
///
/// The enum is `#[non_exhaustive]`: new variants may be added in minor
/// releases, so a `match` on it needs a wildcard arm.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GraphError {
    /// Entity with the given id was not found in the store.
    #[error("Entity '{0}' not found")]
    EntityNotFound(String),

    /// A relationship between two entities with the given type was not found.
    #[error("Relationship '{rel}' from '{from}' to '{to}' not found")]
    RelationshipNotFound {
        /// Source entity id.
        from: String,
        /// Target entity id.
        to: String,
        /// Relationship type.
        rel: String,
    },

    /// An entity with this id already exists.
    #[error("Duplicate entity id: '{0}'")]
    DuplicateEntity(String),

    /// A relationship with this exact triple already exists.
    #[error("Duplicate relationship: '{rel}' from '{from}' to '{to}' already exists")]
    DuplicateRelationship {
        /// Source entity id.
        from: String,
        /// Target entity id.
        to: String,
        /// Relationship type.
        rel: String,
    },

    /// A property was accessed with the wrong type.
    #[error("Property type mismatch: expected {expected}, found {found}")]
    TypeMismatch {
        /// The variant the caller asked for.
        expected: String,
        /// The variant actually stored.
        found: String,
    },

    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A property value was invalid.
    #[error("Invalid property value: {0}")]
    InvalidProperty(String),
}
