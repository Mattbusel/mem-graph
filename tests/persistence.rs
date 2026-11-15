//! serde impls, JSON and binary save/load.

use std::path::PathBuf;

use chrono::{Duration, TimeZone, Utc};
use mem_graph::{Entity, EntityId, GraphError, GraphSnapshot, GraphStore, PropValue, Relationship};
use serde::{Deserialize, Serialize};

fn id(s: &str) -> EntityId {
    EntityId::new(s)
}

/// A graph that uses every property type, temporal bounds and a self-loop.
fn rich_graph() -> GraphStore {
    let t0 = Utc.with_ymd_and_hms(2024, 3, 1, 12, 0, 0).unwrap();
    let mut g = GraphStore::new();
    g.add_entity(
        Entity::new(id("alice"), "Person")
            .with_prop("name", PropValue::Text("Alice \"Al\" Ünal\n".into()))
            .with_prop("age", PropValue::Number(36.5))
            .with_prop("active", PropValue::Bool(true))
            .with_prop("joined", PropValue::Timestamp(t0))
            .with_prop("tags", PropValue::List(vec![PropValue::Text("x".into()), PropValue::Number(-0.25)])),
    )
    .unwrap();
    g.add_entity(Entity::new(id("acme"), "Company")).unwrap();
    g.add_entity(Entity::new(id("loop"), "Node")).unwrap();
    g.add_relationship(
        Relationship::new(id("alice"), id("acme"), "works_at")
            .with_temporal(t0, Some(t0 + Duration::days(365)))
            .with_prop("role", PropValue::Text("eng".into())),
    )
    .unwrap();
    g.add_relationship(Relationship::new(id("alice"), id("acme"), "invested_in").with_temporal(t0, None)).unwrap();
    g.add_relationship(Relationship::new(id("loop"), id("loop"), "self")).unwrap();
    g
}

/// A path in the temp dir unique to this test and process.
fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mem-graph-test-{}-{name}", std::process::id()))
}

#[test]
fn store_json_roundtrip_is_lossless_and_deterministic() {
    let g = rich_graph();
    let json = g.to_json().unwrap();
    let back = GraphStore::from_json(&json).unwrap();
    assert_eq!(back.snapshot(), g.snapshot());
    assert_eq!(back.to_json().unwrap(), json);

    let pretty = g.to_json_pretty().unwrap();
    assert!(pretty.contains('\n'));
    assert_eq!(GraphStore::from_json(&pretty).unwrap().snapshot(), g.snapshot());
}

#[test]
fn graph_store_implements_serde_and_nests_in_user_types() {
    #[derive(Serialize, Deserialize)]
    struct AgentState {
        name: String,
        memory: GraphStore,
    }
    let state = AgentState { name: "scout".into(), memory: rich_graph() };
    let json = serde_json::to_string(&state).unwrap();
    // The store serializes as its snapshot.
    let as_value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(as_value["memory"], serde_json::to_value(state.memory.snapshot()).unwrap());

    let back: AgentState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "scout");
    assert_eq!(back.memory.snapshot(), state.memory.snapshot());
}

#[test]
fn deserializing_an_inconsistent_graph_is_a_serde_error() {
    let dangling = r#"{"entities":[],"relationships":[{"from":"a","to":"b","rel_type":"r","properties":{},
        "valid_from":null,"valid_to":null,"created_at":"2024-01-01T00:00:00Z"}]}"#;
    let err = serde_json::from_str::<GraphStore>(dangling).unwrap_err();
    assert!(err.to_string().contains("'a' not found"), "{err}");
    assert!(matches!(GraphStore::from_json(dangling), Err(GraphError::EntityNotFound(_))));
    assert!(matches!(GraphStore::from_json("{"), Err(GraphError::Serialization(_))));
}

#[test]
fn entities_and_relationships_serialize_on_their_own() {
    let g = rich_graph();
    let alice = g.get_entity(&id("alice")).unwrap();
    let back: Entity = serde_json::from_str(&serde_json::to_string(alice).unwrap()).unwrap();
    assert_eq!(&back, alice);
    let rel = g.get_relationship(&id("alice"), &id("acme"), "works_at").unwrap();
    let back: Relationship = serde_json::from_str(&serde_json::to_string(rel).unwrap()).unwrap();
    assert_eq!(&back, rel);
}

#[test]
fn write_and_read_json_through_io() {
    let g = rich_graph();
    let mut buf = Vec::new();
    g.write_json(&mut buf).unwrap();
    assert_eq!(buf, g.to_json().unwrap().into_bytes());
    let back = GraphStore::read_json(buf.as_slice()).unwrap();
    assert_eq!(back.snapshot(), g.snapshot());

    let snap = GraphSnapshot::read_json(buf.as_slice()).unwrap();
    assert_eq!(snap, g.snapshot());
}

#[test]
fn write_json_reports_writer_failures_as_io_errors() {
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let err = rich_graph().write_json(Broken).unwrap_err();
    assert!(matches!(err, GraphError::Io(ref e) if e.to_string().contains("disk full")), "{err:?}");
}

#[test]
fn save_and_load_json_file() {
    let path = temp_path("save.json");
    let g = rich_graph();
    g.save_json(&path).unwrap();
    assert_eq!(GraphStore::load_json(&path).unwrap().snapshot(), g.snapshot());

    // Saving again replaces the file and leaves no temporary file behind.
    let mut smaller = g.clone();
    smaller.remove_entity(&id("loop")).unwrap();
    smaller.save_json(&path).unwrap();
    assert_eq!(GraphStore::load_json(&path).unwrap().entity_count(), 2);
    let dir = path.parent().unwrap();
    let prefix = format!(".{}", path.file_name().unwrap().to_string_lossy());
    let leftovers = std::fs::read_dir(dir)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(&prefix))
        .count();
    assert_eq!(leftovers, 0);
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn file_errors_are_io_errors() {
    let missing = temp_path("does-not-exist.json");
    assert!(matches!(GraphStore::load_json(&missing), Err(GraphError::Io(_))));
    let no_dir = temp_path("no-such-dir").join("g.json");
    assert!(matches!(rich_graph().save_json(&no_dir), Err(GraphError::Io(_))));
}

#[cfg(feature = "binary")]
mod binary {
    use super::*;

    #[test]
    fn binary_roundtrip_is_lossless_and_smaller_than_json() {
        let g = rich_graph();
        let bytes = g.to_bytes().unwrap();
        assert!(bytes.starts_with(b"MGRAPH\x01"));
        let back = GraphStore::from_bytes(&bytes).unwrap();
        assert_eq!(back.snapshot(), g.snapshot());
        assert_eq!(back.to_bytes().unwrap(), bytes, "deterministic");
        assert!(bytes.len() < g.to_json().unwrap().len());
        assert_eq!(GraphSnapshot::from_bytes(&g.snapshot().to_bytes().unwrap()).unwrap(), g.snapshot());

        let empty = GraphStore::new().to_bytes().unwrap();
        assert_eq!(GraphStore::from_bytes(&empty).unwrap().entity_count(), 0);
    }

    #[test]
    fn binary_rejects_foreign_truncated_and_padded_bytes() {
        let bytes = rich_graph().to_bytes().unwrap();
        let is_binary_err = |r: Result<GraphStore, GraphError>| matches!(r, Err(GraphError::Binary(_)));
        assert!(is_binary_err(GraphStore::from_bytes(b"{\"entities\":[]}")));
        assert!(is_binary_err(GraphStore::from_bytes(b"")));
        assert!(is_binary_err(GraphStore::from_bytes(b"MGRAPH")));
        let mut future = bytes.clone();
        future[6] = 2;
        match GraphStore::from_bytes(&future) {
            Err(GraphError::Binary(msg)) => assert!(msg.contains("version 2"), "{msg}"),
            other => panic!("expected a version error, got {other:?}"),
        }
        for cut in [7, bytes.len() / 2, bytes.len() - 1] {
            assert!(is_binary_err(GraphStore::from_bytes(&bytes[..cut])), "cut at {cut}");
        }
        let mut padded = bytes.clone();
        padded.push(0);
        assert!(is_binary_err(GraphStore::from_bytes(&padded)));
    }

    #[test]
    fn save_and_load_binary_file() {
        let path = temp_path("save.bin");
        let g = rich_graph();
        g.save_binary(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), g.to_bytes().unwrap());
        assert_eq!(GraphStore::load_binary(&path).unwrap().snapshot(), g.snapshot());
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(GraphStore::load_binary(&path), Err(GraphError::Io(_))));
    }

    #[test]
    fn binary_handles_a_larger_graph() {
        let mut g = GraphStore::new();
        for i in 0..2_000 {
            g.add_entity(Entity::new(EntityId::new(format!("n{i}")), "N").with_prop("i", PropValue::Number(i as f64)))
                .unwrap();
        }
        for i in 1..2_000 {
            let rel = Relationship::new(EntityId::new(format!("n{}", i / 2)), EntityId::new(format!("n{i}")), "child");
            g.add_relationship(rel).unwrap();
        }
        let back = GraphStore::from_bytes(&g.to_bytes().unwrap()).unwrap();
        assert_eq!((back.entity_count(), back.edge_count()), (2_000, 1_999));
        assert_eq!(back.snapshot(), g.snapshot());
    }
}
