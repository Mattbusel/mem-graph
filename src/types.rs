// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::error::GraphError;

/// Unique entity identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    /// Create an EntityId from any string.
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }

    /// Create a random UUID-based EntityId.
    pub fn random() -> Self { Self(Uuid::new_v4().to_string()) }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

/// Typed property value for entities and edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropValue {
    /// UTF-8 text value.
    Text(String),
    /// IEEE 754 double-precision float.
    Number(f64),
    /// Boolean flag.
    Bool(bool),
    /// UTC timestamp.
    Timestamp(DateTime<Utc>),
    /// Ordered list of property values.
    List(Vec<PropValue>),
}

impl PropValue {
    /// Return the variant name as a static string (used in error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            PropValue::Text(_) => "Text",
            PropValue::Number(_) => "Number",
            PropValue::Bool(_) => "Bool",
            PropValue::Timestamp(_) => "Timestamp",
            PropValue::List(_) => "List",
        }
    }

    /// Extract a text value, returning TypeMismatch on wrong variant.
    pub fn as_text(&self) -> Result<&str, GraphError> {
        match self {
            PropValue::Text(s) => Ok(s),
            other => Err(GraphError::TypeMismatch {
                expected: "Text".into(),
                found: other.type_name().into(),
            }),
        }
    }

    /// Extract a numeric value, returning TypeMismatch on wrong variant.
    pub fn as_number(&self) -> Result<f64, GraphError> {
        match self {
            PropValue::Number(n) => Ok(*n),
            other => Err(GraphError::TypeMismatch {
                expected: "Number".into(),
                found: other.type_name().into(),
            }),
        }
    }

    /// Extract a boolean value, returning TypeMismatch on wrong variant.
    pub fn as_bool(&self) -> Result<bool, GraphError> {
        match self {
            PropValue::Bool(b) => Ok(*b),
            other => Err(GraphError::TypeMismatch {
                expected: "Bool".into(),
                found: other.type_name().into(),
            }),
        }
    }
}

/// An entity node in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Unique identifier for this entity.
    pub id: EntityId,
    /// Semantic type label (e.g. "Person", "Document").
    pub kind: String,
    /// Typed key-value properties.
    pub properties: std::collections::HashMap<String, PropValue>,
    /// Wall-clock creation time.
    pub created_at: DateTime<Utc>,
}

impl Entity {
    /// Create a new entity with no properties.
    pub fn new(id: EntityId, kind: impl Into<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            properties: std::collections::HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Builder-style property setter.
    pub fn with_prop(mut self, key: impl Into<String>, value: PropValue) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    /// Look up a property by key.
    pub fn get_prop(&self, key: &str) -> Option<&PropValue> {
        self.properties.get(key)
    }
}

/// A directed, typed relationship (edge) between two entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    /// Source entity.
    pub from: EntityId,
    /// Target entity.
    pub to: EntityId,
    /// Semantic relationship type (e.g. "knows", "owns").
    pub rel_type: String,
    /// Typed key-value properties on the edge.
    pub properties: std::collections::HashMap<String, PropValue>,
    /// Optional validity window start.
    pub valid_from: Option<DateTime<Utc>>,
    /// Optional validity window end (exclusive).
    pub valid_to: Option<DateTime<Utc>>,
    /// Wall-clock creation time.
    pub created_at: DateTime<Utc>,
}

impl Relationship {
    /// Create a new relationship with no properties or temporal bounds.
    pub fn new(from: EntityId, to: EntityId, rel_type: impl Into<String>) -> Self {
        Self {
            from,
            to,
            rel_type: rel_type.into(),
            properties: std::collections::HashMap::new(),
            valid_from: None,
            valid_to: None,
            created_at: Utc::now(),
        }
    }

    /// Attach a temporal validity window.
    pub fn with_temporal(mut self, valid_from: DateTime<Utc>, valid_to: Option<DateTime<Utc>>) -> Self {
        self.valid_from = Some(valid_from);
        self.valid_to = valid_to;
        self
    }

    /// Return true if this relationship is valid at time `t`.
    pub fn is_valid_at(&self, t: DateTime<Utc>) -> bool {
        let after_start = self.valid_from.is_none_or(|s| t >= s);
        let before_end = self.valid_to.is_none_or(|e| t < e);
        after_start && before_end
    }

    /// Builder-style property setter.
    pub fn with_prop(mut self, key: impl Into<String>, value: PropValue) -> Self {
        self.properties.insert(key.into(), value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_new_has_correct_kind() {
        let e = Entity::new(EntityId::new("e1"), "Person");
        assert_eq!(e.kind, "Person");
    }

    #[test]
    fn test_entity_with_prop_stores_value() {
        let e = Entity::new(EntityId::new("e1"), "Person")
            .with_prop("name", PropValue::Text("Alice".into()));
        assert_eq!(e.get_prop("name").unwrap().as_text().unwrap(), "Alice");
    }

    #[test]
    fn test_prop_value_type_mismatch_text_returns_error() {
        let v = PropValue::Number(42.0);
        let err = v.as_text().unwrap_err();
        assert!(matches!(err, GraphError::TypeMismatch { .. }));
    }

    #[test]
    fn test_prop_value_type_mismatch_bool_returns_error() {
        let v = PropValue::Text("yes".into());
        assert!(matches!(v.as_bool(), Err(GraphError::TypeMismatch { .. })));
    }

    #[test]
    fn test_prop_value_type_mismatch_number_returns_error() {
        let v = PropValue::Bool(true);
        assert!(matches!(v.as_number(), Err(GraphError::TypeMismatch { .. })));
    }

    #[test]
    fn test_prop_value_as_text_ok() {
        let v = PropValue::Text("hello".into());
        assert_eq!(v.as_text().unwrap(), "hello");
    }

    #[test]
    fn test_prop_value_as_number_ok() {
        let v = PropValue::Number(3.14);
        assert!((v.as_number().unwrap() - 3.14).abs() < 1e-9);
    }

    #[test]
    fn test_prop_value_as_bool_ok() {
        let v = PropValue::Bool(false);
        assert!(!v.as_bool().unwrap());
    }

    #[test]
    fn test_entity_id_display() {
        let id = EntityId::new("abc");
        assert_eq!(format!("{}", id), "abc");
    }

    #[test]
    fn test_entity_id_random_is_unique() {
        let a = EntityId::random();
        let b = EntityId::random();
        assert_ne!(a, b);
    }

    #[test]
    fn test_relationship_temporal_valid_at_mid() {
        let from = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2024-12-31T23:59:59Z").unwrap().with_timezone(&Utc);
        let mid = DateTime::parse_from_rfc3339("2024-06-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let r = Relationship::new(EntityId::new("a"), EntityId::new("b"), "knows")
            .with_temporal(from, Some(to));
        assert!(r.is_valid_at(mid));
    }

    #[test]
    fn test_relationship_temporal_invalid_after_end() {
        let from = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2021-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let after = DateTime::parse_from_rfc3339("2022-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let r = Relationship::new(EntityId::new("a"), EntityId::new("b"), "knows")
            .with_temporal(from, Some(to));
        assert!(!r.is_valid_at(after));
    }

    #[test]
    fn test_relationship_no_temporal_bounds_always_valid() {
        let r = Relationship::new(EntityId::new("a"), EntityId::new("b"), "r");
        assert!(r.is_valid_at(Utc::now()));
    }

    #[test]
    fn test_prop_value_type_name() {
        assert_eq!(PropValue::Text("".into()).type_name(), "Text");
        assert_eq!(PropValue::Number(0.0).type_name(), "Number");
        assert_eq!(PropValue::Bool(true).type_name(), "Bool");
        assert_eq!(PropValue::List(vec![]).type_name(), "List");
    }
}
