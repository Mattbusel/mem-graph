// SPDX-License-Identifier: MIT
use mem_graph::{Entity, EntityId, GraphStore, PropValue, Relationship};

fn person(id: &str, name: &str) -> Entity {
    Entity::new(EntityId::new(id), "Person").with_prop("name", PropValue::Text(name.into()))
}

#[test]
fn test_social_graph_shortest_path_three_hops() {
    let mut g = GraphStore::new();
    g.add_entity(person("alice", "Alice")).unwrap();
    g.add_entity(person("bob", "Bob")).unwrap();
    g.add_entity(person("carol", "Carol")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("alice"), EntityId::new("bob"), "knows")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("carol"), "knows")).unwrap();

    let path = g.shortest_path(&EntityId::new("alice"), &EntityId::new("carol"))
        .unwrap()
        .unwrap();
    assert_eq!(path.len(), 3);
    assert_eq!(path[0].0, "alice");
    assert_eq!(path[2].0, "carol");
}

#[test]
fn test_social_graph_bfs_visits_all() {
    let mut g = GraphStore::new();
    g.add_entity(person("alice", "Alice")).unwrap();
    g.add_entity(person("bob", "Bob")).unwrap();
    g.add_entity(person("carol", "Carol")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("alice"), EntityId::new("bob"), "knows")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("bob"), EntityId::new("carol"), "knows")).unwrap();

    let bfs = g.bfs(&EntityId::new("alice"), 5).unwrap();
    assert_eq!(bfs.len(), 3);
}

#[test]
fn test_transitive_closure_complete_chain() {
    let mut g = GraphStore::new();
    for id in ["a", "b", "c", "d"] {
        g.add_entity(Entity::new(EntityId::new(id), "N")).unwrap();
    }
    g.add_relationship(Relationship::new(EntityId::new("a"), EntityId::new("b"), "r")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("b"), EntityId::new("c"), "r")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("c"), EntityId::new("d"), "r")).unwrap();

    let closure = g.transitive_closure(&EntityId::new("a")).unwrap();
    assert_eq!(closure.len(), 4);
    for id in ["a", "b", "c", "d"] {
        assert!(closure.contains(id), "expected {id} in closure");
    }
}

#[test]
fn test_bidirectional_indexing_out_and_in() {
    let mut g = GraphStore::new();
    g.add_entity(Entity::new(EntityId::new("src"), "N")).unwrap();
    g.add_entity(Entity::new(EntityId::new("dst"), "N")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("src"), EntityId::new("dst"), "edge")).unwrap();

    let out = g.neighbors_out(&EntityId::new("src"));
    let inc = g.neighbors_in(&EntityId::new("dst"));
    assert_eq!(out.len(), 1);
    assert_eq!(inc.len(), 1);
    assert_eq!(out[0].0.id.0, "dst");
    assert_eq!(inc[0].0.id.0, "src");
}

#[test]
fn test_dfs_and_bfs_agree_on_visited_count() {
    let mut g = GraphStore::new();
    for id in ["x", "y", "z"] {
        g.add_entity(Entity::new(EntityId::new(id), "N")).unwrap();
    }
    g.add_relationship(Relationship::new(EntityId::new("x"), EntityId::new("y"), "r")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("y"), EntityId::new("z"), "r")).unwrap();

    let bfs = g.bfs(&EntityId::new("x"), 10).unwrap();
    let dfs = g.dfs(&EntityId::new("x"), 10).unwrap();
    assert_eq!(bfs.len(), dfs.len());
    assert_eq!(bfs.len(), 3);
}

#[test]
fn test_prop_value_stored_and_retrieved_correctly() {
    let e = Entity::new(EntityId::new("e"), "Doc")
        .with_prop("score", PropValue::Number(9.5))
        .with_prop("active", PropValue::Bool(true));
    assert!((e.get_prop("score").unwrap().as_number().unwrap() - 9.5).abs() < 1e-9);
    assert!(e.get_prop("active").unwrap().as_bool().unwrap());
}

#[test]
fn test_no_path_between_disconnected_nodes() {
    let mut g = GraphStore::new();
    g.add_entity(Entity::new(EntityId::new("a"), "N")).unwrap();
    g.add_entity(Entity::new(EntityId::new("b"), "N")).unwrap();
    // No edge added
    let path = g.shortest_path(&EntityId::new("a"), &EntityId::new("b")).unwrap();
    assert!(path.is_none());
}

#[test]
fn test_multi_edge_types_between_same_nodes() {
    let mut g = GraphStore::new();
    g.add_entity(Entity::new(EntityId::new("a"), "N")).unwrap();
    g.add_entity(Entity::new(EntityId::new("b"), "N")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("a"), EntityId::new("b"), "knows")).unwrap();
    g.add_relationship(Relationship::new(EntityId::new("a"), EntityId::new("b"), "works_with")).unwrap();
    assert_eq!(g.edge_count(), 2);
    assert_eq!(g.neighbors_out(&EntityId::new("a")).len(), 2);
}
