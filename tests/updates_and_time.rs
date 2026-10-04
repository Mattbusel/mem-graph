//! Deletes, upserts, snapshots, point-in-time traversal and petgraph interop.

use chrono::{Duration, Utc};
use mem_graph::serial::GraphSnapshot;
use mem_graph::{Entity, EntityId, GraphError, GraphStore, PropValue, Relationship};

fn id(s: &str) -> EntityId {
    EntityId::new(s)
}

fn people() -> GraphStore {
    let mut g = GraphStore::new();
    for (name, kind) in [("alice", "Person"), ("bob", "Person"), ("acme", "Company"), ("globex", "Company"), ("berlin", "City")] {
        g.add_entity(Entity::new(id(name), kind)).unwrap();
    }
    g.add_relationship(Relationship::new(id("alice"), id("bob"), "knows")).unwrap();
    g.add_relationship(Relationship::new(id("bob"), id("acme"), "works_at")).unwrap();
    g.add_relationship(Relationship::new(id("acme"), id("berlin"), "located_in")).unwrap();
    g
}

#[test]
fn removing_an_entity_removes_its_relationships_everywhere() {
    let mut g = people();
    let removed = g.remove_entity(&id("bob")).unwrap();
    assert_eq!(removed.kind, "Person");
    assert_eq!((g.entity_count(), g.edge_count()), (4, 1));
    assert!(g.neighbors_out(&id("alice")).is_empty(), "alice -> bob is gone");
    assert!(g.neighbors_in(&id("acme")).is_empty(), "bob -> acme is gone");
    assert!(matches!(g.remove_entity(&id("bob")), Err(GraphError::EntityNotFound(_))));
    // The id can be reused, with no stale edges.
    g.add_entity(Entity::new(id("bob"), "Robot")).unwrap();
    assert!(g.neighbors_out(&id("bob")).is_empty());
}

#[test]
fn removing_a_relationship_updates_both_indexes() {
    let mut g = people();
    g.remove_relationship(&id("bob"), &id("acme"), "works_at").unwrap();
    assert!(g.neighbors_out(&id("bob")).is_empty());
    assert!(g.neighbors_in(&id("acme")).is_empty());
    assert_eq!(g.shortest_path(&id("alice"), &id("berlin")).unwrap(), None);
    assert!(g.remove_relationship(&id("bob"), &id("acme"), "works_at").is_err());
    // It can be added again.
    g.add_relationship(Relationship::new(id("bob"), id("acme"), "works_at")).unwrap();
    assert_eq!(g.shortest_path(&id("alice"), &id("berlin")).unwrap().unwrap().len(), 4);
}

#[test]
fn upsert_replaces_properties_and_keeps_edges() {
    let mut g = people();
    let old = g.upsert_entity(Entity::new(id("bob"), "Person").with_prop("title", PropValue::Text("CTO".into())));
    assert!(old.is_some());
    assert_eq!(g.get_entity(&id("bob")).unwrap().get_prop("title").unwrap().as_text().unwrap(), "CTO");
    assert_eq!(g.neighbors_out(&id("bob")).len(), 1);
    assert!(g.upsert_entity(Entity::new(id("carol"), "Person")).is_none());
}

#[test]
fn snapshot_round_trips_the_whole_graph() {
    let g = people();
    let json = g.snapshot().to_json().unwrap();
    let mut restored = GraphStore::new();
    GraphSnapshot::from_json(&json).unwrap().restore_into(&mut restored).unwrap();
    assert_eq!((restored.entity_count(), restored.edge_count()), (5, 3));
    assert_eq!(
        restored.shortest_path(&id("alice"), &id("berlin")).unwrap(),
        g.shortest_path(&id("alice"), &id("berlin")).unwrap()
    );
}

#[test]
fn traversals_at_a_time_see_the_graph_as_it_was() {
    let mut g = people();
    let now = Utc::now();
    let last_year = now - Duration::days(365);
    // Bob worked at Globex until last month, and at Acme only since then.
    g.add_relationship(
        Relationship::new(id("bob"), id("globex"), "works_at").with_temporal(now - Duration::days(1000), Some(now - Duration::days(30))),
    )
    .unwrap();
    g.remove_relationship(&id("bob"), &id("acme"), "works_at").unwrap();
    g.add_relationship(Relationship::new(id("bob"), id("acme"), "works_at").with_temporal(now - Duration::days(30), None)).unwrap();

    let employer = |t| -> Vec<String> {
        g.neighbors_out_at(&id("bob"), t).into_iter().map(|(e, _)| e.id.0.clone()).collect()
    };
    assert_eq!(employer(last_year), vec!["globex"]);
    assert_eq!(employer(now), vec!["acme"]);

    assert_eq!(g.shortest_path_at(&id("alice"), &id("berlin"), last_year).unwrap(), None, "acme link did not exist yet");
    assert_eq!(g.shortest_path_at(&id("alice"), &id("berlin"), now).unwrap().unwrap().len(), 4);
    let then: Vec<String> = g.bfs_at(&id("alice"), 5, last_year).unwrap().into_iter().map(|e| e.0).collect();
    assert_eq!(then, vec!["alice", "bob", "globex"]);
    // The untimed traversal still sees every edge.
    assert_eq!(g.bfs(&id("alice"), 5).unwrap().len(), 5);
}

#[cfg(feature = "petgraph")]
mod petgraph_interop {
    use super::*;

    fn road(g: &mut GraphStore, a: &str, b: &str, km: f64) {
        g.add_relationship(Relationship::new(id(a), id(b), "road").with_prop("km", PropValue::Number(km))).unwrap();
    }

    #[test]
    fn weighted_shortest_path_uses_edge_weights() {
        let mut g = GraphStore::new();
        for c in ["a", "b", "c", "d"] {
            g.add_entity(Entity::new(id(c), "City")).unwrap();
        }
        road(&mut g, "a", "d", 100.0); // direct but long
        road(&mut g, "a", "b", 10.0);
        road(&mut g, "b", "c", 10.0);
        road(&mut g, "c", "d", 10.0);
        let (km, path) = g.shortest_path_weighted(&id("a"), &id("d"), "km").unwrap().unwrap();
        assert_eq!(km, 30.0);
        assert_eq!(path, vec![id("a"), id("b"), id("c"), id("d")]);
        // Unweighted BFS takes the single hop.
        assert_eq!(g.shortest_path(&id("a"), &id("d")).unwrap().unwrap().len(), 2);

        road(&mut g, "d", "a", -5.0);
        assert!(matches!(g.shortest_path_weighted(&id("a"), &id("d"), "km"), Err(GraphError::InvalidProperty(_))));
    }

    #[test]
    fn petgraph_algorithms_run_on_the_store_and_it_converts_back() {
        let mut g = people();
        g.add_relationship(Relationship::new(id("berlin"), id("alice"), "home_of")).unwrap();
        let (pg, nodes) = g.to_petgraph();
        let sccs = petgraph::algo::tarjan_scc(&pg);
        let biggest = sccs.iter().map(Vec::len).max().unwrap();
        assert_eq!(biggest, 4, "alice -> bob -> acme -> berlin -> alice is one cycle");
        assert!(petgraph::algo::toposort(&pg, None).is_err(), "the cycle blocks a topological order");
        assert_eq!(pg[nodes[&id("acme")]].kind, "Company");

        let back = GraphStore::from_petgraph(&pg).unwrap();
        assert_eq!((back.entity_count(), back.edge_count()), (g.entity_count(), g.edge_count()));
        assert!(back.get_relationship(&id("berlin"), &id("alice"), "home_of").is_ok());
    }
}
