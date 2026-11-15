//! Conversions to and from a plain `petgraph::Graph`.
#![cfg(feature = "petgraph")]

use mem_graph::{Entity, EntityId, GraphError, GraphStore, PropValue, Relationship};
use petgraph::graph::{DiGraph, NodeIndex};

fn id(s: &str) -> EntityId {
    EntityId::new(s)
}

fn sample() -> GraphStore {
    let mut g = GraphStore::new();
    for (key, kind) in [("c", "Task"), ("a", "Task"), ("b", "Task"), ("d", "Task")] {
        g.add_entity(Entity::new(id(key), kind).with_prop("cost", PropValue::Number(key.len() as f64))).unwrap();
    }
    for (f, t, r) in [("a", "b", "blocks"), ("b", "c", "blocks"), ("a", "c", "blocks"), ("a", "c", "mentions")] {
        g.add_relationship(Relationship::new(id(f), id(t), r).with_prop("w", PropValue::Number(2.0))).unwrap();
    }
    g
}

#[test]
fn to_petgraph_graph_is_deterministic_and_complete() {
    let g = sample();
    let (pg, nodes) = g.to_petgraph_graph();
    assert_eq!((pg.node_count(), pg.edge_count()), (4, 4));
    let order: Vec<&str> = pg.node_indices().map(|n| pg[n].id.as_str()).collect();
    assert_eq!(order, ["a", "b", "c", "d"], "nodes in id order");
    for (key, &idx) in &nodes {
        assert_eq!(&pg[idx].id, key);
    }
    let edges: Vec<(&str, &str, &str)> = pg
        .raw_edges()
        .iter()
        .map(|e| (pg[e.source()].id.as_str(), pg[e.target()].id.as_str(), e.weight.rel_type.as_str()))
        .collect();
    assert_eq!(edges, [("a", "b", "blocks"), ("a", "c", "blocks"), ("a", "c", "mentions"), ("b", "c", "blocks")]);

    // The same graph built in another order converts to the same indices.
    let mut other = GraphStore::new();
    for e in g.snapshot().entities.into_iter().rev() {
        other.add_entity(e).unwrap();
    }
    for r in g.snapshot().relationships.into_iter().rev() {
        other.add_relationship(r).unwrap();
    }
    assert_eq!(other.to_petgraph_graph().1, nodes);
}

#[test]
fn petgraph_algorithms_run_on_the_plain_graph() {
    let g = sample();
    let (pg, nodes) = g.to_petgraph_graph();
    let topo: Vec<&str> = petgraph::algo::toposort(&pg, None).unwrap().iter().map(|&n| pg[n].id.as_str()).collect();
    let pos = |s: &str| topo.iter().position(|x| *x == s).unwrap();
    assert!(pos("a") < pos("b") && pos("b") < pos("c"));
    let costs = petgraph::algo::dijkstra(&pg, nodes[&id("a")], None, |e| {
        e.weight().properties["w"].as_number().unwrap()
    });
    assert_eq!(costs[&nodes[&id("c")]], 2.0);
    assert!(!costs.contains_key(&nodes[&id("d")]));
}

#[test]
fn plain_graph_roundtrip_is_lossless() {
    let g = sample();
    let (pg, _) = g.to_petgraph_graph();
    let back = GraphStore::from_petgraph_graph(&pg).unwrap();
    assert_eq!(back.snapshot(), g.snapshot());
}

#[test]
fn from_petgraph_graph_uses_graph_endpoints_over_stale_weights() {
    let mut pg: DiGraph<Entity, Relationship> = DiGraph::new();
    let x = pg.add_node(Entity::new(id("x"), "N"));
    let y = pg.add_node(Entity::new(id("y"), "N"));
    // The weight claims ghost -> ghost; the edge says x -> y.
    pg.add_edge(x, y, Relationship::new(id("ghost"), id("ghost"), "r"));
    let g = GraphStore::from_petgraph_graph(&pg).unwrap();
    assert!(g.get_relationship(&id("x"), &id("y"), "r").is_ok());
}

#[test]
fn from_petgraph_graph_rejects_duplicates() {
    let mut pg: DiGraph<Entity, Relationship> = DiGraph::new();
    pg.add_node(Entity::new(id("x"), "N"));
    pg.add_node(Entity::new(id("x"), "N"));
    assert!(matches!(GraphStore::from_petgraph_graph(&pg), Err(GraphError::DuplicateEntity(_))));

    let mut pg: DiGraph<Entity, Relationship> = DiGraph::new();
    let x = pg.add_node(Entity::new(id("x"), "N"));
    let y = pg.add_node(Entity::new(id("y"), "N"));
    pg.add_edge(x, y, Relationship::new(id("x"), id("y"), "r"));
    pg.add_edge(x, y, Relationship::new(id("x"), id("y"), "r"));
    assert!(matches!(GraphStore::from_petgraph_graph(&pg), Err(GraphError::DuplicateRelationship { .. })));
}

#[test]
fn from_petgraph_with_imports_arbitrary_weights() {
    // A graph with u32 indices, string nodes and weighted string edges.
    let mut pg: petgraph::Graph<&str, (&str, f64), petgraph::Directed, u32> = petgraph::Graph::default();
    let rust = pg.add_node("Rust");
    let llvm = pg.add_node("LLVM");
    let cpp = pg.add_node("C++");
    pg.add_edge(rust, llvm, ("compiles_with", 0.9));
    pg.add_edge(llvm, cpp, ("written_in", 1.0));

    let g = GraphStore::from_petgraph_with(
        &pg,
        |idx: NodeIndex<u32>, name| {
            Entity::new(EntityId::new(name.to_lowercase()), "Tech")
                .with_prop("name", PropValue::Text((*name).into()))
                .with_prop("index", PropValue::Number(idx.index() as f64))
        },
        |(label, weight), from, to| Relationship::new(from, to, *label).with_prop("confidence", PropValue::Number(*weight)),
    )
    .unwrap();
    assert_eq!((g.entity_count(), g.edge_count()), (3, 2));
    let rel = g.get_relationship(&id("rust"), &id("llvm"), "compiles_with").unwrap();
    assert_eq!(rel.properties["confidence"].as_number().unwrap(), 0.9);
    assert_eq!(g.get_entity(&id("c++")).unwrap().properties["index"].as_number().unwrap(), 2.0);
    assert_eq!(
        g.shortest_path(&id("rust"), &id("c++")).unwrap().unwrap(),
        [id("rust"), id("llvm"), id("c++")]
    );

    // Whatever endpoints the closure puts on the relationship, the graph's win.
    let g = GraphStore::from_petgraph_with(
        &pg,
        |_, name| Entity::new(EntityId::new(*name), "Tech"),
        |(label, _), _, _| Relationship::new(id("nope"), id("nope"), *label),
    )
    .unwrap();
    assert!(g.get_relationship(&id("LLVM"), &id("C++"), "written_in").is_ok());
}

#[test]
fn stable_graph_conversion_is_now_deterministic_too() {
    let g = sample();
    let (sg, nodes) = g.to_petgraph();
    let order: Vec<&str> = sg.node_indices().map(|n| sg[n].id.as_str()).collect();
    assert_eq!(order, ["a", "b", "c", "d"]);
    assert_eq!(sg[nodes[&id("d")]].id.as_str(), "d");
}
