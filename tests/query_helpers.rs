//! Query helpers: neighbors by type, filtered shortest paths, subgraphs and
//! text search.

use chrono::{Duration, Utc};
use mem_graph::{Direction, Entity, EntityId, GraphError, GraphStore, PropValue, Relationship};

fn id(s: &str) -> EntityId {
    EntityId::new(s)
}

fn ids(list: &[&Entity]) -> Vec<String> {
    list.iter().map(|e| e.id.0.clone()).collect()
}

fn text(s: &str) -> PropValue {
    PropValue::Text(s.into())
}

/// alice -knows-> bob -works_at-> acme -located_in-> berlin
/// carol -works_at-> acme, alice -reports_to-> carol -reports_to-> dave
/// bob -knows-> alice (a cycle through "knows")
fn office() -> GraphStore {
    let mut g = GraphStore::new();
    let people = [("alice", "Alice Smith"), ("bob", "Bob Ross"), ("carol", "Carol Danvers"), ("dave", "Dave Grohl")];
    for (key, name) in people {
        g.add_entity(Entity::new(id(key), "Person").with_prop("name", text(name))).unwrap();
    }
    g.add_entity(
        Entity::new(id("acme"), "Company")
            .with_prop("name", text("ACME Corp"))
            .with_prop("tags", PropValue::List(vec![text("rockets"), PropValue::Number(7.0), text("Anvils")]))
            .with_prop("employees", PropValue::Number(120.0)),
    )
    .unwrap();
    g.add_entity(Entity::new(id("berlin"), "City")).unwrap();
    for (f, t, r) in [
        ("alice", "bob", "knows"),
        ("bob", "alice", "knows"),
        ("bob", "acme", "works_at"),
        ("carol", "acme", "works_at"),
        ("acme", "berlin", "located_in"),
        ("alice", "carol", "reports_to"),
        ("carol", "dave", "reports_to"),
    ] {
        g.add_relationship(Relationship::new(id(f), id(t), r)).unwrap();
    }
    g
}

#[test]
fn neighbors_by_relationship_type_in_each_direction() {
    let g = office();
    assert_eq!(ids(&g.neighbors_out_by_type(&id("bob"), "works_at")), ["acme"]);
    assert!(g.neighbors_out_by_type(&id("bob"), "reports_to").is_empty());
    let mut staff = ids(&g.neighbors_in_by_type(&id("acme"), "works_at"));
    staff.sort();
    assert_eq!(staff, ["bob", "carol"]);
    // alice knows bob and bob knows alice: with Both, bob is listed once.
    assert_eq!(ids(&g.neighbors_by_type(&id("alice"), "knows", Direction::Both)), ["bob"]);
    assert_eq!(ids(&g.neighbors_by_type(&id("carol"), "reports_to", Direction::Both)), ["dave", "alice"]);
    assert!(g.neighbors_out_by_type(&id("nobody"), "knows").is_empty());
}

#[test]
fn relationships_of_an_entity_and_of_a_type() {
    let g = office();
    let out: Vec<&str> = g.relationships_of(&id("bob"), Direction::Outgoing).iter().map(|r| r.rel_type.as_str()).collect();
    assert_eq!(out, ["knows", "works_at"]);
    assert_eq!(g.relationships_of(&id("acme"), Direction::Incoming).len(), 2);
    assert_eq!(g.relationships_of(&id("acme"), Direction::Both).len(), 3);
    assert!(g.relationships_of(&id("nobody"), Direction::Both).is_empty());

    let chain: Vec<(&str, &str)> =
        g.relationships_of_type("reports_to").iter().map(|r| (r.from.as_str(), r.to.as_str())).collect();
    assert_eq!(chain, [("alice", "carol"), ("carol", "dave")]);
    assert_eq!(ids(&g.entities_of_kind("Person")), ["alice", "bob", "carol", "dave"]);
    assert!(g.entities_of_kind("Planet").is_empty());
}

#[test]
fn shortest_path_via_only_uses_the_given_relationship_types() {
    let g = office();
    // alice -> carol -> dave through reports_to.
    let p = g.shortest_path_via(&id("alice"), &id("dave"), &["reports_to"]).unwrap().unwrap();
    assert_eq!(p, [id("alice"), id("carol"), id("dave")]);
    // berlin is reachable from alice, but not through "knows" alone.
    assert!(g.shortest_path(&id("alice"), &id("berlin")).unwrap().is_some());
    assert!(g.shortest_path_via(&id("alice"), &id("berlin"), &["knows"]).unwrap().is_none());
    let p = g.shortest_path_via(&id("alice"), &id("berlin"), &["knows", "works_at", "located_in"]).unwrap().unwrap();
    assert_eq!(p, [id("alice"), id("bob"), id("acme"), id("berlin")]);
    assert_eq!(g.shortest_path_via(&id("bob"), &id("bob"), &[]).unwrap().unwrap(), [id("bob")]);
    assert!(matches!(
        g.shortest_path_via(&id("ghost"), &id("bob"), &["knows"]),
        Err(GraphError::EntityNotFound(ref x)) if x == "ghost"
    ));
}

#[test]
fn shortest_path_where_can_go_against_the_arrows() {
    let g = office();
    // There is no directed path from berlin to anyone.
    assert!(g.shortest_path(&id("berlin"), &id("carol")).unwrap().is_none());
    let back = g.shortest_path_where(&id("berlin"), &id("carol"), Direction::Incoming, |_| true).unwrap().unwrap();
    assert_eq!(back, [id("berlin"), id("acme"), id("carol")]);
    // Undirected: dave to bob through any relationship.
    let any = g.shortest_path_where(&id("dave"), &id("bob"), Direction::Both, |_| true).unwrap().unwrap();
    assert_eq!(any.len(), 4, "dave - carol - (alice or acme) - bob: {any:?}");
    assert_eq!((any[0].as_str(), any[3].as_str()), ("dave", "bob"));
    // Outgoing with an always-true predicate matches shortest_path.
    for (a, b) in [("alice", "berlin"), ("carol", "dave"), ("berlin", "alice")] {
        assert_eq!(
            g.shortest_path_where(&id(a), &id(b), Direction::Outgoing, |_| true).unwrap(),
            g.shortest_path(&id(a), &id(b)).unwrap()
        );
    }
}

#[test]
fn shortest_path_where_with_a_time_predicate_matches_shortest_path_at() {
    let mut g = office();
    let now = Utc::now();
    let rel = g.remove_relationship(&id("bob"), &id("acme"), "works_at").unwrap();
    g.add_relationship(rel.with_temporal(now - Duration::days(10), Some(now - Duration::days(1)))).unwrap();
    for t in [now, now - Duration::days(5)] {
        assert_eq!(
            g.shortest_path_where(&id("alice"), &id("berlin"), Direction::Outgoing, |r| r.is_valid_at(t)).unwrap(),
            g.shortest_path_at(&id("alice"), &id("berlin"), t).unwrap()
        );
    }
    // Today bob no longer works at acme, so the path goes through carol.
    let today = g.shortest_path_at(&id("alice"), &id("berlin"), now).unwrap().unwrap();
    assert_eq!(today, [id("alice"), id("carol"), id("acme"), id("berlin")]);
    let then = g.shortest_path_at(&id("alice"), &id("berlin"), now - Duration::days(5)).unwrap().unwrap();
    assert_eq!(then, [id("alice"), id("bob"), id("acme"), id("berlin")]);
}

#[test]
fn within_by_direction() {
    let g = office();
    assert_eq!(g.within(&id("alice"), 3, Direction::Outgoing).unwrap(), g.bfs(&id("alice"), 3).unwrap());
    assert_eq!(g.within(&id("acme"), 0, Direction::Both).unwrap(), [id("acme")]);
    let mut up = g.within(&id("acme"), 1, Direction::Incoming).unwrap();
    up.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(up, [id("acme"), id("bob"), id("carol")]);
    assert_eq!(g.within(&id("berlin"), 10, Direction::Both).unwrap().len(), 6, "the graph is connected");
    assert!(g.within(&id("ghost"), 1, Direction::Both).is_err());
}

#[test]
fn subgraph_extracts_the_induced_neighborhood() {
    let g = office();
    let sub = g.subgraph(&id("acme"), 1, Direction::Both).unwrap();
    let mut names: Vec<String> = sub.entities().map(|e| e.id.0.clone()).collect();
    names.sort();
    assert_eq!(names, ["acme", "berlin", "bob", "carol"]);
    // bob->acme, carol->acme, acme->berlin; no edges to alice or dave.
    assert_eq!(sub.edge_count(), 3);
    // Entity data comes along unchanged.
    assert_eq!(sub.get_entity(&id("acme")).unwrap(), g.get_entity(&id("acme")).unwrap());

    // Induced: alice and bob are 1 hop out from alice, and the bob -> alice
    // edge between them comes along too.
    let sub = g.subgraph(&id("alice"), 1, Direction::Outgoing).unwrap();
    assert_eq!(sub.entity_count(), 3);
    assert!(sub.get_relationship(&id("bob"), &id("alice"), "knows").is_ok());
    assert!(sub.get_relationship(&id("alice"), &id("carol"), "reports_to").is_ok());

    // Depth 0 is the start alone; the subgraph is an independent store.
    let mut solo = g.subgraph(&id("bob"), 0, Direction::Both).unwrap();
    assert_eq!((solo.entity_count(), solo.edge_count()), (1, 0));
    solo.add_entity(Entity::new(id("new"), "X")).unwrap();
    assert!(g.get_entity(&id("new")).is_err());

    assert!(matches!(g.subgraph(&id("ghost"), 2, Direction::Both), Err(GraphError::EntityNotFound(_))));
}

#[test]
fn induced_subgraph_from_a_list_of_ids() {
    let g = office();
    let sub = g.induced_subgraph(&[id("alice"), id("bob"), id("alice")]).unwrap();
    assert_eq!((sub.entity_count(), sub.edge_count()), (2, 2));
    assert!(matches!(
        g.induced_subgraph(&[id("alice"), id("ghost")]),
        Err(GraphError::EntityNotFound(ref x)) if x == "ghost"
    ));
    let empty = g.induced_subgraph(&[]).unwrap();
    assert_eq!(empty.entity_count(), 0);
}

#[test]
fn text_search_matches_ids_kinds_and_text_properties_case_insensitively() {
    let g = office();
    assert_eq!(ids(&g.search_entities("ross")), ["bob"]);
    assert_eq!(ids(&g.search_entities("ROSS")), ["bob"]);
    assert_eq!(ids(&g.search_entities("acme")), ["acme"], "matches the id and the name once");
    assert_eq!(ids(&g.search_entities("anvil")), ["acme"], "text inside a list property");
    assert_eq!(ids(&g.search_entities("city")), ["berlin"], "the kind");
    assert_eq!(ids(&g.search_entities("e")).len(), 6, "every entity has an 'e' in its id, kind or name");
    assert_eq!(ids(&g.search_entities("a")), ["acme", "alice", "carol", "dave"]);
    assert!(g.search_entities("120").is_empty(), "numbers are not searched");
    assert!(g.search_entities("zzz").is_empty());
    assert_eq!(g.search_entities("").len(), g.entity_count());

    assert_eq!(ids(&g.search_entities_by_prop("name", "corp")), ["acme"]);
    assert!(g.search_entities_by_prop("tags", "smith").is_empty());
    assert_eq!(ids(&g.search_entities_by_prop("name", "D")), ["carol", "dave"]);
}

#[test]
fn text_search_handles_non_ascii_case() {
    let mut g = GraphStore::new();
    g.add_entity(Entity::new(id("m"), "City").with_prop("name", text("MÜNCHEN"))).unwrap();
    g.add_entity(Entity::new(id("z"), "City").with_prop("name", text("Zürich"))).unwrap();
    assert_eq!(ids(&g.search_entities("münchen")), ["m"]);
    assert_eq!(ids(&g.search_entities("ZÜR")), ["z"]);
}

#[test]
fn find_by_prop_is_exact() {
    let g = office();
    assert_eq!(ids(&g.find_by_prop("employees", &PropValue::Number(120.0))), ["acme"]);
    assert!(g.find_by_prop("employees", &text("120")).is_empty());
    assert_eq!(ids(&g.find_by_prop("name", &text("Bob Ross"))), ["bob"]);
    assert!(g.find_by_prop("name", &text("bob ross")).is_empty());
}
